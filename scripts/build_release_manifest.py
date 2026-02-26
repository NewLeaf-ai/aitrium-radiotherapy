#!/usr/bin/env python3
"""
Build release manifest.json for aitrium-radiotherapy distribution assets.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
from pathlib import Path


TARGET_ARCHIVES = {
    "darwin-aarch64": "aitrium-radiotherapy-server-darwin-aarch64.tar.gz",
    "darwin-x86_64": "aitrium-radiotherapy-server-darwin-x86_64.tar.gz",
    "linux-x86_64": "aitrium-radiotherapy-server-linux-x86_64.tar.gz",
    "windows-x86_64": "aitrium-radiotherapy-server-windows-x86_64.zip",
}


def read_checksum(checksum_path: Path) -> str:
    raw = checksum_path.read_text(encoding="utf-8").strip()
    if not raw:
        raise ValueError(f"Empty checksum file: {checksum_path}")
    return raw.split()[0]


def read_smoke_report(report_path: Path, target: str) -> bool:
    if not report_path.exists():
        raise FileNotFoundError(f"Missing smoke report: {report_path}")

    report = json.loads(report_path.read_text(encoding="utf-8"))
    report_target = report.get("target")
    if report_target != target:
        raise ValueError(
            f"Smoke report target mismatch for {target}: found {report_target!r}"
        )

    passed = report.get("passed")
    if passed is not True:
        raise ValueError(f"Smoke report did not pass for target {target}: {report_path}")
    return True


def build_manifest(
    assets_dir: Path,
    version: str,
    channel: str,
    base_url: str,
    commit_sha: str,
    build_id: str,
) -> dict:
    published_at = dt.datetime.now(tz=dt.timezone.utc).isoformat().replace("+00:00", "Z")
    targets = []

    for target, archive_name in TARGET_ARCHIVES.items():
        archive_path = assets_dir / archive_name
        checksum_path = assets_dir / f"{archive_name}.sha256"
        smoke_report_name = f"smoke-report-{target}.json"
        smoke_report_path = assets_dir / smoke_report_name
        if not archive_path.exists():
            raise FileNotFoundError(f"Missing archive: {archive_path}")
        if not checksum_path.exists():
            raise FileNotFoundError(f"Missing checksum: {checksum_path}")
        smoke_passed = read_smoke_report(smoke_report_path, target)

        targets.append(
            {
                "target": target,
                "archive": archive_name,
                "checksum": read_checksum(checksum_path),
                "url": f"{base_url}/{archive_name}",
                "checksum_url": f"{base_url}/{archive_name}.sha256",
                "smoke_passed": smoke_passed,
                "smoke_report_url": f"{base_url}/{smoke_report_name}",
            }
        )

    skill_archive = "aitrium-radiotherapy-skill.tar.gz"
    install_sh = "install.sh"
    install_ps1 = "install.ps1"

    for required in [skill_archive, install_sh, install_ps1]:
        if not (assets_dir / required).exists():
            raise FileNotFoundError(f"Missing required asset: {required}")
        if not (assets_dir / f"{required}.sha256").exists():
            raise FileNotFoundError(f"Missing required checksum: {required}.sha256")

    manifest = {
        "version": version,
        "channel": channel,
        "commit_sha": commit_sha,
        "build_id": build_id,
        "published_at": published_at,
        "targets": targets,
        "skill": {
            "archive": skill_archive,
            "checksum": read_checksum(assets_dir / f"{skill_archive}.sha256"),
            "url": f"{base_url}/{skill_archive}",
            "checksum_url": f"{base_url}/{skill_archive}.sha256",
        },
        "installers": {
            "sh": {
                "file": install_sh,
                "checksum": read_checksum(assets_dir / f"{install_sh}.sha256"),
                "url": f"{base_url}/{install_sh}",
                "checksum_url": f"{base_url}/{install_sh}.sha256",
            },
            "ps1": {
                "file": install_ps1,
                "checksum": read_checksum(assets_dir / f"{install_ps1}.sha256"),
                "url": f"{base_url}/{install_ps1}",
                "checksum_url": f"{base_url}/{install_ps1}.sha256",
            },
        },
    }
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--assets-dir", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--channel", choices=["stable", "beta"], required=True)
    parser.add_argument("--base-url", required=True)
    parser.add_argument("--commit-sha", required=True)
    parser.add_argument("--build-id", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    assets_dir = Path(args.assets_dir)
    output = Path(args.output)
    manifest = build_manifest(
        assets_dir=assets_dir,
        version=args.version,
        channel=args.channel,
        base_url=args.base_url.rstrip("/"),
        commit_sha=args.commit_sha,
        build_id=args.build_id,
    )
    output.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"Wrote {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
