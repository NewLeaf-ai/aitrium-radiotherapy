import { access } from "node:fs/promises";
import { constants } from "node:fs";
import { spawnSync } from "node:child_process";

import { describe, expect, it } from "vitest";

import { AitriumRadiotherapyClient } from "../src/client";
import { MissingFileError } from "../src/errors";

async function hasBinary(path: string): Promise<boolean> {
  if (!path.includes("/")) {
    const which = spawnSync("command", ["-v", path], { shell: true });
    return which.status === 0;
  }

  try {
    await access(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

describe("AitriumRadiotherapyClient integration", () => {
  it("lists expected tools", async () => {
    const binary = process.env.AITRIUM_RADIOTHERAPY_SERVER_BIN ?? "aitrium-radiotherapy-server";
    if (!(await hasBinary(binary)) && binary === "aitrium-radiotherapy-server") {
      return;
    }

    const client = new AitriumRadiotherapyClient([binary]);
    try {
      const tools = await client.listTools();
      const names = tools.map((tool) => tool.name);
      expect(names).toContain("rt_inspect");
      expect(names).toContain("rt_dvh");
      expect(names).toContain("rt_dvh_metrics");
      expect(names).toContain("rt_anonymize_metadata");
      expect(names).toContain("rt_anonymize_template_get");
      expect(names).toContain("rt_anonymize_template_update");
      expect(names).toContain("rt_anonymize_template_reset");
    } finally {
      await client.close();
    }
  });

  it("maps tool errors to typed exceptions", async () => {
    const binary = process.env.AITRIUM_RADIOTHERAPY_SERVER_BIN ?? "aitrium-radiotherapy-server";
    if (!(await hasBinary(binary)) && binary === "aitrium-radiotherapy-server") {
      return;
    }

    const client = new AitriumRadiotherapyClient([binary]);
    try {
      await expect(client.inspect("/definitely/missing/path")).rejects.toBeInstanceOf(MissingFileError);
    } finally {
      await client.close();
    }
  });
});
