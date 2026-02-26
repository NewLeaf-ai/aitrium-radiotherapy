import { ApiError, ErrorCode } from "./types";

export class AitriumRadiotherapyError extends Error {
  public readonly apiError: ApiError;

  constructor(apiError: ApiError) {
    super(`${apiError.code}: ${apiError.message}`);
    this.name = "AitriumRadiotherapyError";
    this.apiError = apiError;
  }
}

export class InvalidInputError extends AitriumRadiotherapyError {}
export class MissingFileError extends AitriumRadiotherapyError {}
export class DicomParseError extends AitriumRadiotherapyError {}
export class MatchingError extends AitriumRadiotherapyError {}
export class ComputeError extends AitriumRadiotherapyError {}
export class InternalError extends AitriumRadiotherapyError {}

export function throwMappedError(apiError: ApiError): never {
  const mapping: Record<ErrorCode, new (error: ApiError) => AitriumRadiotherapyError> = {
    INVALID_INPUT: InvalidInputError,
    FILE_NOT_FOUND: MissingFileError,
    DICOM_PARSE_ERROR: DicomParseError,
    MATCHING_ERROR: MatchingError,
    COMPUTE_ERROR: ComputeError,
    INTERNAL: InternalError
  };

  const ErrorCtor = mapping[apiError.code] ?? AitriumRadiotherapyError;
  throw new ErrorCtor(apiError);
}
