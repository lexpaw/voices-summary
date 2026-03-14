import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  AppStatus,
  AudioRecord,
  AudioRecordDetail,
  AudioRecordQuery,
  LogEntry
} from "../types/models";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const mockSettings: AppSettings = {
  dataDir: "D:\\VoicesSummary\\data",
  outputDir: "D:\\VoicesSummary\\output",
  scanDirectories: ["Record"],
  allowedExtensions: ["wav", "mp3", "m4a", "aac"],
  deviceMatchRule: {
    vid: "1234",
    pid: "5678",
    volumeLabel: "VOICE_RECORDER",
    pathHints: ["Record"]
  },
  transcriptionProvider: {
    baseUrl: "https://example.local/transcribe",
    apiKey: "",
    authHeader: "Authorization",
    extraHeaders: []
  },
  summaryProvider: {
    baseUrl: "https://example.local/summarize",
    apiKey: "",
    authHeader: "Authorization",
    extraHeaders: []
  },
  scanIntervalSecs: 15,
  requestTimeoutSecs: 90,
  maxRetries: 3,
  processingConcurrency: 2
};

const mockRecords: AudioRecordDetail[] = [
  {
    id: "sample-audio-1",
    deviceIdentifier: "USB:1234:5678:VOICE_RECORDER",
    fileName: "2026-03-13-meeting.wav",
    relativePath: "Record/2026-03-13-meeting.wav",
    importedPath: "D:\\VoicesSummary\\data\\raw\\sample-audio-1.wav",
    transcriptPath: "D:\\VoicesSummary\\output\\transcripts\\sample-audio-1.md",
    summaryPath: "D:\\VoicesSummary\\output\\summaries\\sample-audio-1.md",
    status: "completed",
    syncedAt: "2026-03-13T08:15:00Z",
    updatedAt: "2026-03-13T08:18:00Z",
    transcriptSegments: [
      {
        speakerLabel: "Speaker 1",
        startMs: 0,
        endMs: 11500,
        text: "我们先确认本周录音笔同步和转写结果。"
      },
      {
        speakerLabel: "Speaker 2",
        startMs: 11600,
        endMs: 22000,
        text: "摘要里要保留结论和待办事项。"
      }
    ],
    summary: {
      title: "同步检查会议",
      bullets: ["确认录音文件已自动同步", "摘要需要突出结论与行动项"],
      fullText: "本次会话主要围绕录音文件同步结果和摘要结构展开，明确需要保留关键结论与后续待办。"
    }
  }
];

const mockLogs: LogEntry[] = [
  {
    id: 1,
    level: "INFO",
    scope: "device",
    message: "模拟环境：使用浏览器模式，未连接 Tauri 后端。",
    createdAt: "2026-03-13T08:20:00Z"
  }
];

async function call<T>(command: string, payload?: Record<string, unknown>): Promise<T> {
  if (!isTauri) {
    return mockCall<T>(command, payload);
  }
  return invoke<T>(command, payload);
}

async function mockCall<T>(command: string, payload?: Record<string, unknown>): Promise<T> {
  switch (command) {
    case "get_app_status":
      return {
        deviceConnected: true,
        connectedDevices: [
          {
            identifier: "USB:1234:5678:VOICE_RECORDER",
            volumeLabel: "VOICE_RECORDER",
            driveLetter: "E:",
            pnpDeviceId: "USBSTOR\\DISK&VEN_SAMPLE&PROD_RECORDER\\123456",
            lastSeenAt: "2026-03-13T08:18:00Z"
          }
        ],
        lastScanAt: "2026-03-13T08:18:00Z",
        totalFiles: mockRecords.length,
        completedFiles: 1,
        inFlightFiles: 0,
        failedFiles: 0,
        recentFailures: []
      } as T;
    case "list_audio_records": {
      const query = (payload?.query ?? {}) as AudioRecordQuery;
      const filtered = mockRecords.filter((record) => {
        const matchStatus = !query.status || query.status === "all" || record.status === query.status;
        const keyword = query.search?.trim().toLowerCase();
        const matchSearch =
          !keyword ||
          record.fileName.toLowerCase().includes(keyword) ||
          record.relativePath.toLowerCase().includes(keyword);
        return matchStatus && matchSearch;
      });
      return filtered.map(({ transcriptSegments, summary, ...record }) => record) as T;
    }
    case "get_audio_record_detail":
      return mockRecords.find((item) => item.id === payload?.id) as T;
    case "get_settings":
      return structuredClone(mockSettings) as T;
    case "save_settings":
      Object.assign(mockSettings, payload?.settings);
      return structuredClone(mockSettings) as T;
    case "list_logs":
      return mockLogs as T;
    case "retry_job":
    case "trigger_scan":
    case "open_path":
      return undefined as T;
    default:
      throw new Error(`Unknown mock command: ${command}`);
  }
}

export const api = {
  getAppStatus: () => call<AppStatus>("get_app_status"),
  listAudioRecords: (query: AudioRecordQuery) => call<AudioRecord[]>("list_audio_records", { query }),
  getAudioRecordDetail: (id: string) => call<AudioRecordDetail>("get_audio_record_detail", { id }),
  getSettings: () => call<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => call<AppSettings>("save_settings", { settings }),
  triggerScan: () => call<void>("trigger_scan"),
  retryJob: (id: string, stage: "transcription" | "summary") => call<void>("retry_job", { id, stage }),
  openPath: (kind: "data_dir" | "output_dir" | "audio" | "transcript" | "summary", id?: string) =>
    call<void>("open_path", { kind, id }),
  listLogs: () => call<LogEntry[]>("list_logs")
};
