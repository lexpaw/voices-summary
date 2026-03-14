export type AudioStatus =
  | "imported"
  | "transcribing"
  | "transcribed"
  | "summarizing"
  | "completed"
  | "failed_transcription"
  | "failed_summary";

export interface DeviceInfo {
  identifier: string;
  volumeLabel?: string | null;
  driveLetter: string;
  pnpDeviceId?: string | null;
  lastSeenAt: string;
}

export interface FailureItem {
  audioId: string;
  fileName: string;
  status: AudioStatus;
  errorMessage?: string | null;
  updatedAt: string;
}

export interface AppStatus {
  deviceConnected: boolean;
  connectedDevices: DeviceInfo[];
  lastScanAt?: string | null;
  totalFiles: number;
  completedFiles: number;
  inFlightFiles: number;
  failedFiles: number;
  recentFailures: FailureItem[];
}

export interface TranscriptSegment {
  speakerLabel: string;
  startMs: number;
  endMs: number;
  text: string;
}

export interface SummaryDocument {
  title?: string | null;
  bullets: string[];
  fullText: string;
}

export interface AudioRecord {
  id: string;
  deviceIdentifier: string;
  fileName: string;
  relativePath: string;
  importedPath: string;
  transcriptPath?: string | null;
  summaryPath?: string | null;
  status: AudioStatus;
  errorMessage?: string | null;
  syncedAt: string;
  updatedAt: string;
}

export interface AudioRecordDetail extends AudioRecord {
  transcriptSegments: TranscriptSegment[];
  summary?: SummaryDocument | null;
}

export interface AudioRecordQuery {
  search?: string;
  status?: AudioStatus | "all";
}

export interface HeaderEntry {
  key: string;
  value: string;
}

export interface ProviderSettings {
  baseUrl: string;
  apiKey?: string;
  authHeader?: string;
  extraHeaders: HeaderEntry[];
}

export interface DeviceMatchRule {
  vid?: string;
  pid?: string;
  volumeLabel?: string;
  pathHints: string[];
}

export interface AppSettings {
  dataDir: string;
  outputDir: string;
  scanDirectories: string[];
  allowedExtensions: string[];
  deviceMatchRule: DeviceMatchRule;
  transcriptionProvider: ProviderSettings;
  summaryProvider: ProviderSettings;
  scanIntervalSecs: number;
  requestTimeoutSecs: number;
  maxRetries: number;
  processingConcurrency: number;
}

export interface LogEntry {
  id: number;
  level: string;
  scope: string;
  message: string;
  createdAt: string;
}
