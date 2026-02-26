from __future__ import annotations

from aitrium_radiotherapy_client.models import ApiErrorModel, ErrorCode


class TransportError(RuntimeError):
    pass


class AitriumRadiotherapyError(Exception):
    def __init__(self, error: ApiErrorModel):
        super().__init__(f"{error.code.value}: {error.message}")
        self.error = error


class InvalidInputError(AitriumRadiotherapyError):
    pass


class FileNotFoundError(AitriumRadiotherapyError):
    pass


class DicomParseError(AitriumRadiotherapyError):
    pass


class MatchingError(AitriumRadiotherapyError):
    pass


class ComputeError(AitriumRadiotherapyError):
    pass


class InternalError(AitriumRadiotherapyError):
    pass


def raise_for_error(error: ApiErrorModel) -> None:
    mapping = {
        ErrorCode.INVALID_INPUT: InvalidInputError,
        ErrorCode.FILE_NOT_FOUND: FileNotFoundError,
        ErrorCode.DICOM_PARSE_ERROR: DicomParseError,
        ErrorCode.MATCHING_ERROR: MatchingError,
        ErrorCode.COMPUTE_ERROR: ComputeError,
        ErrorCode.INTERNAL: InternalError,
    }
    exc = mapping.get(error.code, AitriumRadiotherapyError)
    raise exc(error)
