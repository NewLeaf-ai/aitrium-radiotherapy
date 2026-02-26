from __future__ import annotations

import shutil

import pytest

from aitrium_radiotherapy_client import FileNotFoundError, AitriumRadiotherapyClient


@pytest.fixture()
def client() -> AitriumRadiotherapyClient:
    binary = shutil.which("aitrium-radiotherapy-server")
    if binary is None:
        pytest.skip("aitrium-radiotherapy-server binary is not on PATH")

    cli = AitriumRadiotherapyClient(command=[binary], auto_initialize=True)
    try:
        yield cli
    finally:
        cli.close()


def test_list_tools(client: AitriumRadiotherapyClient) -> None:
    tools = client.list_tools()
    names = {tool.name for tool in tools}
    assert "rt_inspect" in names
    assert "rt_dvh" in names
    assert "rt_dvh_metrics" in names


def test_inspect_missing_path_returns_typed_error(client: AitriumRadiotherapyClient) -> None:
    with pytest.raises(FileNotFoundError):
        client.inspect("/does/not/exist")
