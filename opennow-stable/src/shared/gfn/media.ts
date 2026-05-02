export interface ScreenshotSaveRequest {
  dataUrl: string;
  gameTitle?: string;
}

export interface ScreenshotDeleteRequest {
  id: string;
}

export interface ScreenshotSaveAsRequest {
  id: string;
}

export interface ScreenshotSaveAsResult {
  saved: boolean;
  filePath?: string;
}

export interface ScreenshotEntry {
  id: string;
  fileName: string;
  filePath: string;
  createdAtMs: number;
  sizeBytes: number;
  dataUrl: string;
}

export interface RecordingEntry {
  id: string;
  fileName: string;
  filePath: string;
  createdAtMs: number;
  sizeBytes: number;
  durationMs: number;
  gameTitle?: string;
  thumbnailDataUrl?: string;
}

export interface RecordingBeginRequest {
  mimeType: string;
}

export interface RecordingBeginResult {
  recordingId: string;
}

export interface RecordingChunkRequest {
  recordingId: string;
  chunk: ArrayBuffer;
}

export interface RecordingFinishRequest {
  recordingId: string;
  durationMs: number;
  gameTitle?: string;
  thumbnailDataUrl?: string;
}

export interface RecordingAbortRequest {
  recordingId: string;
}

export interface RecordingDeleteRequest {
  id: string;
}

export interface MediaListingEntry {
  id: string;
  fileName: string;
  filePath: string;
  createdAtMs: number;
  sizeBytes: number;
  gameTitle?: string;
  durationMs?: number;
  thumbnailDataUrl?: string;
  dataUrl?: string;
}

export interface MediaListingResult {
  screenshots: MediaListingEntry[];
  videos: MediaListingEntry[];
}
