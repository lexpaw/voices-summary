<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { api } from "./lib/api";
import type {
  AppSettings,
  AppStatus,
  AudioRecord,
  AudioRecordDetail,
  AudioStatus,
  LogEntry
} from "./types/models";

type TabKey = "overview" | "records" | "settings" | "logs";

const tabs: { key: TabKey; label: string }[] = [
  { key: "overview", label: "总览" },
  { key: "records", label: "记录" },
  { key: "settings", label: "设置" },
  { key: "logs", label: "日志" }
];

const currentTab = ref<TabKey>("overview");
const status = ref<AppStatus | null>(null);
const records = ref<AudioRecord[]>([]);
const logs = ref<LogEntry[]>([]);
const selectedRecordId = ref<string | null>(null);
const selectedRecordDetail = ref<AudioRecordDetail | null>(null);
const settings = ref<AppSettings | null>(null);
const savingSettings = ref(false);
const scanning = ref(false);
const loadingDetail = ref(false);
const errorMessage = ref("");
const successMessage = ref("");
const pollTimer = ref<number | null>(null);

const filters = reactive({
  search: "",
  status: "all" as AudioStatus | "all"
});

const lastScanText = computed(() => status.value?.lastScanAt ? formatDateTime(status.value.lastScanAt) : "尚未扫描");
const completedRatio = computed(() => {
  if (!status.value || status.value.totalFiles === 0) {
    return 0;
  }
  return Math.round((status.value.completedFiles / status.value.totalFiles) * 100);
});

function resetNotice() {
  errorMessage.value = "";
  successMessage.value = "";
}

function setError(message: string) {
  successMessage.value = "";
  errorMessage.value = message;
}

function setSuccess(message: string) {
  errorMessage.value = "";
  successMessage.value = message;
}

function formatDateTime(value: string) {
  return new Date(value).toLocaleString("zh-CN", { hour12: false });
}

function formatDuration(startMs: number, endMs: number) {
  const totalSeconds = Math.max(0, Math.floor((endMs - startMs) / 1000));
  const minutes = Math.floor(totalSeconds / 60).toString().padStart(2, "0");
  const seconds = (totalSeconds % 60).toString().padStart(2, "0");
  return `${minutes}:${seconds}`;
}

async function loadStatus() {
  status.value = await api.getAppStatus();
}

async function loadRecords() {
  records.value = await api.listAudioRecords({
    search: filters.search.trim() || undefined,
    status: filters.status
  });
  if (!selectedRecordId.value && records.value.length > 0) {
    selectedRecordId.value = records.value[0].id;
  }
  if (selectedRecordId.value && !records.value.some((item) => item.id === selectedRecordId.value)) {
    selectedRecordId.value = records.value[0]?.id ?? null;
  }
}

async function loadDetail(id: string) {
  loadingDetail.value = true;
  try {
    selectedRecordDetail.value = await api.getAudioRecordDetail(id);
  } finally {
    loadingDetail.value = false;
  }
}

async function loadSettings() {
  settings.value = await api.getSettings();
}

async function loadLogs() {
  logs.value = await api.listLogs();
}

async function refreshAll() {
  try {
    await Promise.all([loadStatus(), loadRecords(), loadSettings(), loadLogs()]);
    if (selectedRecordId.value) {
      await loadDetail(selectedRecordId.value);
    }
  } catch (error) {
    setError(error instanceof Error ? error.message : "刷新失败");
  }
}

async function triggerScan() {
  scanning.value = true;
  resetNotice();
  try {
    await api.triggerScan();
    await refreshAll();
    setSuccess("已触发设备扫描。");
  } catch (error) {
    setError(error instanceof Error ? error.message : "扫描失败");
  } finally {
    scanning.value = false;
  }
}

async function saveSettings() {
  if (!settings.value) {
    return;
  }
  savingSettings.value = true;
  resetNotice();
  try {
    settings.value = await api.saveSettings(settings.value);
    setSuccess("设置已保存。");
  } catch (error) {
    setError(error instanceof Error ? error.message : "保存设置失败");
  } finally {
    savingSettings.value = false;
  }
}

async function retryStage(stage: "transcription" | "summary") {
  if (!selectedRecordId.value) {
    return;
  }
  resetNotice();
  try {
    await api.retryJob(selectedRecordId.value, stage);
    await refreshAll();
    setSuccess(stage === "transcription" ? "已重新排队转写任务。" : "已重新排队摘要任务。");
  } catch (error) {
    setError(error instanceof Error ? error.message : "重试失败");
  }
}

async function openPath(kind: "data_dir" | "output_dir" | "audio" | "transcript" | "summary") {
  try {
    await api.openPath(kind, selectedRecordId.value ?? undefined);
  } catch (error) {
    setError(error instanceof Error ? error.message : "打开路径失败");
  }
}

watch(
  () => selectedRecordId.value,
  async (id) => {
    if (!id) {
      selectedRecordDetail.value = null;
      return;
    }
    await loadDetail(id);
  }
);

watch(
  () => [filters.search, filters.status],
  async () => {
    await loadRecords();
  }
);

onMounted(async () => {
  await refreshAll();
  pollTimer.value = window.setInterval(() => {
    void loadStatus();
    void loadRecords();
    void loadLogs();
    if (selectedRecordId.value) {
      void loadDetail(selectedRecordId.value);
    }
  }, 5000);
});

onBeforeUnmount(() => {
  if (pollTimer.value) {
    window.clearInterval(pollTimer.value);
  }
});
</script>

<template>
  <div class="shell">
    <aside class="sidebar">
      <div class="brand">
        <div class="brand__eyebrow">Windows Tray Console</div>
        <h1>Voices Summary</h1>
        <p>录音笔自动同步、转写与摘要控制台</p>
      </div>

      <nav class="tabs">
        <button
          v-for="tab in tabs"
          :key="tab.key"
          class="tab"
          :class="{ 'tab--active': currentTab === tab.key }"
          @click="currentTab = tab.key"
        >
          {{ tab.label }}
        </button>
      </nav>

      <div class="sidebar__actions">
        <button class="primary-button" :disabled="scanning" @click="triggerScan">
          {{ scanning ? "扫描中..." : "立即扫描设备" }}
        </button>
        <button class="secondary-button" @click="openPath('data_dir')">打开数据目录</button>
        <button class="secondary-button" @click="openPath('output_dir')">打开输出目录</button>
      </div>
    </aside>

    <main class="content">
      <header class="topbar">
        <div>
          <h2>{{ tabs.find((item) => item.key === currentTab)?.label }}</h2>
          <p>最近扫描：{{ lastScanText }}</p>
        </div>
        <div class="notices">
          <div v-if="successMessage" class="notice notice--success">{{ successMessage }}</div>
          <div v-if="errorMessage" class="notice notice--error">{{ errorMessage }}</div>
        </div>
      </header>

      <section v-if="currentTab === 'overview'" class="overview">
        <div class="metric-grid">
          <article class="metric-card">
            <span class="metric-card__label">设备状态</span>
            <strong>{{ status?.deviceConnected ? "已连接" : "未连接" }}</strong>
            <small>{{ status?.connectedDevices.length ?? 0 }} 台匹配设备</small>
          </article>
          <article class="metric-card">
            <span class="metric-card__label">录音总数</span>
            <strong>{{ status?.totalFiles ?? 0 }}</strong>
            <small>累计已发现音频</small>
          </article>
          <article class="metric-card">
            <span class="metric-card__label">完成率</span>
            <strong>{{ completedRatio }}%</strong>
            <small>{{ status?.completedFiles ?? 0 }} 个已完成</small>
          </article>
          <article class="metric-card">
            <span class="metric-card__label">失败任务</span>
            <strong>{{ status?.failedFiles ?? 0 }}</strong>
            <small>{{ status?.inFlightFiles ?? 0 }} 个处理中</small>
          </article>
        </div>

        <div class="overview-grid">
          <section class="panel">
            <div class="panel__header">
              <h3>已连接设备</h3>
            </div>
            <div v-if="status?.connectedDevices.length" class="device-list">
              <article v-for="device in status.connectedDevices" :key="device.identifier" class="device-card">
                <strong>{{ device.volumeLabel || "未命名卷" }}</strong>
                <span>{{ device.driveLetter }}</span>
                <small>{{ device.identifier }}</small>
                <small>{{ formatDateTime(device.lastSeenAt) }}</small>
              </article>
            </div>
            <div v-else class="empty-state">当前没有识别到目标录音笔。</div>
          </section>

          <section class="panel">
            <div class="panel__header">
              <h3>最近失败任务</h3>
            </div>
            <div v-if="status?.recentFailures.length" class="failure-list">
              <article
                v-for="failure in status.recentFailures"
                :key="failure.audioId"
                class="failure-item"
              >
                <div>
                  <strong>{{ failure.fileName }}</strong>
                  <small>{{ failure.status }}</small>
                </div>
                <p>{{ failure.errorMessage || "未知错误" }}</p>
              </article>
            </div>
            <div v-else class="empty-state">暂无失败任务。</div>
          </section>
        </div>
      </section>

      <section v-else-if="currentTab === 'records'" class="records-view">
        <section class="panel records-list">
          <div class="panel__header panel__header--column">
            <h3>音频记录</h3>
            <div class="filters">
              <input v-model="filters.search" type="search" placeholder="搜索文件名或路径" />
              <select v-model="filters.status">
                <option value="all">全部状态</option>
                <option value="imported">已导入</option>
                <option value="transcribing">转写中</option>
                <option value="transcribed">待摘要</option>
                <option value="summarizing">摘要中</option>
                <option value="completed">已完成</option>
                <option value="failed_transcription">转写失败</option>
                <option value="failed_summary">摘要失败</option>
              </select>
            </div>
          </div>

          <div v-if="records.length" class="record-items">
            <button
              v-for="record in records"
              :key="record.id"
              class="record-item"
              :class="{ 'record-item--active': selectedRecordId === record.id }"
              @click="selectedRecordId = record.id"
            >
              <div>
                <strong>{{ record.fileName }}</strong>
                <small>{{ record.relativePath }}</small>
              </div>
              <span class="status-badge" :data-status="record.status">{{ record.status }}</span>
            </button>
          </div>
          <div v-else class="empty-state">没有匹配的音频记录。</div>
        </section>

        <section class="panel record-detail">
          <div v-if="selectedRecordDetail" class="record-detail__content">
            <div class="panel__header">
              <div>
                <h3>{{ selectedRecordDetail.fileName }}</h3>
                <p>{{ selectedRecordDetail.relativePath }}</p>
              </div>
              <span class="status-badge" :data-status="selectedRecordDetail.status">
                {{ selectedRecordDetail.status }}
              </span>
            </div>

            <div class="record-toolbar">
              <button class="secondary-button" @click="openPath('audio')">打开音频文件</button>
              <button class="secondary-button" @click="openPath('transcript')">打开转写稿</button>
              <button class="secondary-button" @click="openPath('summary')">打开摘要</button>
              <button class="secondary-button" @click="retryStage('transcription')">重试转写</button>
              <button class="secondary-button" @click="retryStage('summary')">重试摘要</button>
            </div>

            <audio class="audio-player" :src="`file:///${selectedRecordDetail.importedPath.replace(/\\/g, '/')}`" controls />

            <div class="detail-grid">
              <section class="detail-card">
                <h4>完整转写</h4>
                <div v-if="loadingDetail" class="empty-state">加载中...</div>
                <div v-else-if="selectedRecordDetail.transcriptSegments.length" class="segment-list">
                  <article
                    v-for="(segment, index) in selectedRecordDetail.transcriptSegments"
                    :key="`${segment.startMs}-${index}`"
                    class="segment"
                  >
                    <div class="segment__meta">
                      <strong>{{ segment.speakerLabel }}</strong>
                      <span>{{ formatDuration(segment.startMs, segment.endMs) }}</span>
                    </div>
                    <p>{{ segment.text }}</p>
                  </article>
                </div>
                <div v-else class="empty-state">暂无转写结果。</div>
              </section>

              <section class="detail-card">
                <h4>摘要</h4>
                <div v-if="selectedRecordDetail.summary" class="summary-block">
                  <strong>{{ selectedRecordDetail.summary.title || "未命名摘要" }}</strong>
                  <ul>
                    <li v-for="bullet in selectedRecordDetail.summary.bullets" :key="bullet">{{ bullet }}</li>
                  </ul>
                  <p>{{ selectedRecordDetail.summary.fullText }}</p>
                </div>
                <div v-else class="empty-state">暂无摘要。</div>
              </section>
            </div>

            <div v-if="selectedRecordDetail.errorMessage" class="error-inline">
              {{ selectedRecordDetail.errorMessage }}
            </div>
          </div>
          <div v-else class="empty-state">选择一条录音记录查看详情。</div>
        </section>
      </section>

      <section v-else-if="currentTab === 'settings'" class="settings-view">
        <form v-if="settings" class="settings-form" @submit.prevent="saveSettings">
          <section class="panel">
            <div class="panel__header"><h3>目录与扫描</h3></div>
            <label>
              数据目录
              <input v-model="settings.dataDir" type="text" />
            </label>
            <label>
              输出目录
              <input v-model="settings.outputDir" type="text" />
            </label>
            <label>
              扫描目录（逗号分隔）
              <input
                :value="settings.scanDirectories.join(', ')"
                type="text"
                @input="settings.scanDirectories = ($event.target as HTMLInputElement).value.split(',').map((item) => item.trim()).filter(Boolean)"
              />
            </label>
            <label>
              支持扩展名（逗号分隔）
              <input
                :value="settings.allowedExtensions.join(', ')"
                type="text"
                @input="settings.allowedExtensions = ($event.target as HTMLInputElement).value.split(',').map((item) => item.trim()).filter(Boolean)"
              />
            </label>
          </section>

          <section class="panel">
            <div class="panel__header"><h3>录音笔识别</h3></div>
            <label>
              VID
              <input v-model="settings.deviceMatchRule.vid" type="text" />
            </label>
            <label>
              PID
              <input v-model="settings.deviceMatchRule.pid" type="text" />
            </label>
            <label>
              卷标
              <input v-model="settings.deviceMatchRule.volumeLabel" type="text" />
            </label>
            <label>
              路径特征（逗号分隔）
              <input
                :value="settings.deviceMatchRule.pathHints.join(', ')"
                type="text"
                @input="settings.deviceMatchRule.pathHints = ($event.target as HTMLInputElement).value.split(',').map((item) => item.trim()).filter(Boolean)"
              />
            </label>
          </section>

          <section class="panel">
            <div class="panel__header"><h3>转写服务</h3></div>
            <label>
              Base URL
              <input v-model="settings.transcriptionProvider.baseUrl" type="url" />
            </label>
            <label>
              鉴权 Header
              <input v-model="settings.transcriptionProvider.authHeader" type="text" />
            </label>
            <label>
              API Key / Token
              <input v-model="settings.transcriptionProvider.apiKey" type="password" />
            </label>
          </section>

          <section class="panel">
            <div class="panel__header"><h3>摘要服务</h3></div>
            <label>
              Base URL
              <input v-model="settings.summaryProvider.baseUrl" type="url" />
            </label>
            <label>
              鉴权 Header
              <input v-model="settings.summaryProvider.authHeader" type="text" />
            </label>
            <label>
              API Key / Token
              <input v-model="settings.summaryProvider.apiKey" type="password" />
            </label>
          </section>

          <section class="panel">
            <div class="panel__header"><h3>处理策略</h3></div>
            <label>
              扫描间隔（秒）
              <input v-model.number="settings.scanIntervalSecs" min="5" type="number" />
            </label>
            <label>
              请求超时（秒）
              <input v-model.number="settings.requestTimeoutSecs" min="10" type="number" />
            </label>
            <label>
              最大重试次数
              <input v-model.number="settings.maxRetries" min="0" type="number" />
            </label>
            <label>
              并发任务数
              <input v-model.number="settings.processingConcurrency" min="1" max="4" type="number" />
            </label>
          </section>

          <button class="primary-button" :disabled="savingSettings" type="submit">
            {{ savingSettings ? "保存中..." : "保存设置" }}
          </button>
        </form>
      </section>

      <section v-else class="logs-view">
        <section class="panel">
          <div class="panel__header">
            <h3>任务日志</h3>
          </div>
          <div v-if="logs.length" class="log-list">
            <article v-for="log in logs" :key="log.id" class="log-item">
              <div>
                <strong>{{ log.level }}</strong>
                <small>{{ log.scope }}</small>
              </div>
              <p>{{ log.message }}</p>
              <span>{{ formatDateTime(log.createdAt) }}</span>
            </article>
          </div>
          <div v-else class="empty-state">暂无日志。</div>
        </section>
      </section>
    </main>
  </div>
</template>
