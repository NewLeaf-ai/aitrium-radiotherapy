import { ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import { createInterface } from "node:readline";

import { throwMappedError } from "./errors";
import {
  ApiError,
  RtAnonymizeTemplateGetInput,
  RtAnonymizeTemplateGetResponse,
  RtAnonymizeTemplateResetInput,
  RtAnonymizeTemplateResetResponse,
  RtAnonymizeTemplateUpdateInput,
  RtAnonymizeTemplateUpdateResponse,
  DvhMetricSpec,
  RtAnonymizeMetadataInput,
  RtAnonymizeMetadataResponse,
  RtDvhMetricsResponse,
  RtDvhResponse,
  RtInspectResponse,
  ToolSpec
} from "./types";

interface JsonRpcResponse {
  id: number;
  result?: Record<string, unknown>;
  error?: unknown;
}

interface PendingRequest {
  resolve: (value: Record<string, unknown>) => void;
  reject: (reason: Error) => void;
}

export class AitriumRadiotherapyClient {
  private proc: ChildProcessWithoutNullStreams;
  private nextId = 1;
  private pending = new Map<number, PendingRequest>();

  constructor(command: string[] = ["aitrium-radiotherapy-server"], autoInitialize = true) {
    const [bin, ...args] = command;
    this.proc = spawn(bin, args, { stdio: ["pipe", "pipe", "pipe"] });

    const rl = createInterface({ input: this.proc.stdout });
    rl.on("line", (line) => this.handleLine(line));

    this.proc.on("exit", (code) => {
      const error = new Error(`aitrium-radiotherapy-server exited with code ${code}`);
      for (const pending of this.pending.values()) {
        pending.reject(error);
      }
      this.pending.clear();
    });

    if (autoInitialize) {
      void this.rpc("initialize", {});
    }
  }

  async close(): Promise<void> {
    if (!this.proc.killed) {
      this.proc.kill("SIGTERM");
    }
  }

  async listTools(): Promise<ToolSpec[]> {
    const result = await this.rpc("tools/list", {});
    return (result.tools as ToolSpec[]) ?? [];
  }

  async inspect(path: string): Promise<RtInspectResponse> {
    const payload = await this.callTool("rt_inspect", { path });
    return payload as unknown as RtInspectResponse;
  }

  async dvh(input: {
    rtstruct_path: string;
    rtdose_path: string;
    structures?: string[];
    interpolation?: boolean;
    z_segments?: number;
    include_curves?: boolean;
  }): Promise<RtDvhResponse> {
    const payload = await this.callTool("rt_dvh", input);
    return payload as unknown as RtDvhResponse;
  }

  async dvhMetrics(input: {
    rtstruct_path: string;
    rtdose_path: string;
    metrics: DvhMetricSpec[];
    structures?: string[];
    interpolation?: boolean;
    z_segments?: number;
  }): Promise<RtDvhMetricsResponse> {
    const payload = await this.callTool("rt_dvh_metrics", input);
    return payload as unknown as RtDvhMetricsResponse;
  }

  async anonymizeMetadata(input: RtAnonymizeMetadataInput): Promise<RtAnonymizeMetadataResponse> {
    const payload = await this.callTool("rt_anonymize_metadata", input);
    return payload as unknown as RtAnonymizeMetadataResponse;
  }

  async getAnonymizeTemplate(
    input: RtAnonymizeTemplateGetInput = {}
  ): Promise<RtAnonymizeTemplateGetResponse> {
    const payload = await this.callTool("rt_anonymize_template_get", input);
    return payload as unknown as RtAnonymizeTemplateGetResponse;
  }

  async updateAnonymizeTemplate(
    input: RtAnonymizeTemplateUpdateInput = {}
  ): Promise<RtAnonymizeTemplateUpdateResponse> {
    const payload = await this.callTool("rt_anonymize_template_update", input);
    return payload as unknown as RtAnonymizeTemplateUpdateResponse;
  }

  async resetAnonymizeTemplate(
    input: RtAnonymizeTemplateResetInput = {}
  ): Promise<RtAnonymizeTemplateResetResponse> {
    const payload = await this.callTool("rt_anonymize_template_reset", input);
    return payload as unknown as RtAnonymizeTemplateResetResponse;
  }

  private async callTool(
    name: string,
    argumentsValue: Record<string, unknown>
  ): Promise<Record<string, unknown>> {
    const result = await this.rpc("tools/call", {
      name,
      arguments: argumentsValue
    });

    const isError = Boolean(result.isError);
    let payload = result.structuredContent as Record<string, unknown> | undefined;

    if (!payload) {
      const content = (result.content as Array<Record<string, unknown>>) ?? [];
      const first = content[0] ?? {};
      if ("json" in first && typeof first.json === "object" && first.json !== null) {
        payload = first.json as Record<string, unknown>;
      } else if (first.type === "text" && typeof first.text === "string") {
        try {
          payload = JSON.parse(first.text) as Record<string, unknown>;
        } catch {
          throw new Error("Invalid JSON in text tool payload");
        }
      }
    }

    if (!payload) {
      payload = {};
    }

    if (isError) {
      throwMappedError(payload as unknown as ApiError);
    }

    return payload;
  }

  private rpc(
    method: string,
    params: Record<string, unknown>
  ): Promise<Record<string, unknown>> {
    const id = this.nextId++;

    const payload = {
      jsonrpc: "2.0",
      id,
      method,
      params
    };

    const promise = new Promise<Record<string, unknown>>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
    });

    this.proc.stdin.write(`${JSON.stringify(payload)}\n`);
    return promise;
  }

  private handleLine(line: string): void {
    let response: JsonRpcResponse;
    try {
      response = JSON.parse(line) as JsonRpcResponse;
    } catch {
      return;
    }

    const pending = this.pending.get(response.id);
    if (!pending) {
      return;
    }

    this.pending.delete(response.id);

    if (response.error) {
      pending.reject(new Error(`JSON-RPC error: ${JSON.stringify(response.error)}`));
      return;
    }

    pending.resolve(response.result ?? {});
  }
}
