import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  disable as disableAutostart,
  enable as enableAutostart,
  isEnabled as isAutostartEnabled,
} from "@tauri-apps/plugin-autostart";
import {
  isPermissionGranted,
  requestPermission,
} from "@tauri-apps/plugin-notification";
import { openUrl } from "@tauri-apps/plugin-opener";
import claudeLogo from "./assets/provider-claude.png";
import codexLogo from "./assets/provider-codex.png";
import {
  formatAbsoluteTime,
  formatCountdown,
  formatKrw,
  formatPercent,
  formatRelativeEpoch,
  formatTokens,
  formatUsd,
  riskTone,
  sourceLabel,
} from "./lib/format";
import type {
  AppSnapshot,
  ProviderSnapshot,
  ResetCredits,
  ResetResult,
  UsageWindow,
} from "./types";
import "./App.css";

const EMPTY_SNAPSHOT: AppSnapshot = {
  generatedAtEpochSeconds: 0,
  lastAttemptAtEpochSeconds: 0,
  lastCostScanAtEpochSeconds: 0,
  staleAfterSeconds: 180,
  refreshIntervalSeconds: 60,
  sidecarVersion: "0.59.0",
  providers: [
    {
      id: "codex",
      name: "Codex",
      source: "local",
      windows: [],
      cost: null,
      resetCredits: null,
      lastSuccessAtEpochSeconds: 0,
      stale: true,
      error: "첫 사용량을 읽는 중입니다.",
    },
    {
      id: "claude",
      name: "Claude",
      source: "local",
      windows: [],
      cost: null,
      resetCredits: null,
      lastSuccessAtEpochSeconds: 0,
      stale: true,
      error: "첫 사용량을 읽는 중입니다.",
    },
  ],
  exchangeRate: null,
  globalError: null,
};

const PROVIDER_URLS = {
  codex: "https://chatgpt.com/codex",
  claude: "https://claude.ai/settings/usage",
} as const;

const PROVIDER_LOGOS = {
  codex: codexLogo,
  claude: claudeLogo,
} as const;

type NotificationState = "checking" | "granted" | "not-granted";

function lowestRemaining(provider: ProviderSnapshot): number | null {
  if (provider.windows.length === 0) return null;
  return Math.min(...provider.windows.map((window) => window.remainingPercent));
}

function WindowMeter({ window, nowMs }: { window: UsageWindow; nowMs: number }) {
  const tone = riskTone(window.remainingPercent);
  return (
    <div className="window-meter">
      <div className="window-heading">
        <span>{window.label}</span>
        <strong className={`remaining remaining-${tone}`}>
          {formatPercent(window.remainingPercent)} 남음
        </strong>
      </div>
      <div
        className="meter-track"
        role="progressbar"
        aria-label={`${window.label} 남은 사용량`}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(window.remainingPercent)}
      >
        <span
          className={`meter-fill meter-fill-${tone}`}
          style={{ width: `${Math.min(100, Math.max(0, window.remainingPercent))}%` }}
        />
      </div>
      <div className="window-meta">
        <span>{formatPercent(window.remainingPercent)} 사용 가능</span>
        <span>
          {formatCountdown(window.resetsAt, nowMs)} · {formatAbsoluteTime(window.resetsAt)}
        </span>
      </div>
    </div>
  );
}

function CostBlock({ provider }: { provider: ProviderSnapshot }) {
  if (!provider.cost) {
    return (
      <section className="cost-block cost-empty" aria-label="토큰 비용">
        <p>토큰 비용을 계산하는 중입니다.</p>
      </section>
    );
  }

  const { cost } = provider;
  return (
    <section className="cost-block" aria-label="토큰 비용">
      <div className="section-label-row">
        <h3>토큰 비용</h3>
        <span className="estimate-badge">정가 환산</span>
      </div>
      <div className="cost-grid">
        <div>
          <span className="cost-label">
            하루 사용{cost.recentDate ? ` · ${cost.recentDate.slice(5).replace("-", ".")}` : ""}
          </span>
          <strong>{formatTokens(cost.recentTokens)} 토큰</strong>
          <span>{formatUsd(cost.recentCostUsd)}</span>
        </div>
        <div>
          <span className="cost-label">최근 30일</span>
          <strong>{formatTokens(cost.last30DaysTokens)} 토큰</strong>
          <span>{formatUsd(cost.last30DaysCostUsd)}</span>
        </div>
      </div>
      <p className="cost-note">구독료 외 실제 청구액이 아니라 API 정가 기준 추정치입니다.</p>
    </section>
  );
}

function ProviderCard({
  provider,
  nowMs,
  onOpen,
  onReset,
}: {
  provider: ProviderSnapshot;
  nowMs: number;
  onOpen: () => void;
  onReset: (credits: ResetCredits) => void;
}) {
  const remaining = lowestRemaining(provider);
  const credits = provider.resetCredits;

  return (
    <article className={`provider-card provider-${provider.id}`}>
      <header className="provider-header">
        <div className={`provider-mark provider-mark-${provider.id}`} aria-hidden="true">
          <img src={PROVIDER_LOGOS[provider.id]} alt="" />
        </div>
        <div className="provider-title">
          <div className="provider-name-line">
            <h2>{provider.name}</h2>
            <span className={`health-dot ${provider.stale ? "health-stale" : "health-live"}`} />
          </div>
          <p>
            {sourceLabel(provider.source)} · 정상값 {formatRelativeEpoch(provider.lastSuccessAtEpochSeconds, nowMs)}
          </p>
        </div>
        <div className="headline-usage">
          <strong>{remaining == null ? "—" : formatPercent(remaining)}</strong>
          <span>가장 적게 남음</span>
        </div>
      </header>

      {provider.error && (
        <div className={`status-message ${provider.stale ? "status-warning" : ""}`} role="status">
          <span aria-hidden="true">{provider.stale ? "◷" : "i"}</span>
          <p>
            {provider.error}
            {provider.lastSuccessAtEpochSeconds > 0 && " 마지막 정상값을 유지합니다."}
          </p>
        </div>
      )}

      <section className="windows" aria-label={`${provider.name} 사용 한도`}>
        {provider.windows.length > 0 ? (
          provider.windows.map((window) => (
            <WindowMeter key={window.id} window={window} nowMs={nowMs} />
          ))
        ) : (
          <div className="loading-windows" aria-live="polite">
            <span />
            <span />
            <p>첫 한도를 읽고 있습니다. 최대 30초 정도 걸릴 수 있어요.</p>
          </div>
        )}
      </section>

      <CostBlock provider={provider} />

      {provider.id === "codex" && credits && credits.availableCount > 0 && (
        <section className="reset-credit">
          <div>
            <span className="reset-kicker">보유 초기화권</span>
            <strong>{credits.availableCount}장</strong>
            <p>
              {credits.nearestExpiry
                ? `${formatAbsoluteTime(credits.nearestExpiry)}까지 사용 가능`
                : "만료 시각 미정"}
            </p>
          </div>
          <button className="reset-button" type="button" onClick={() => onReset(credits)}>
            한도 초기화
          </button>
        </section>
      )}

      <button className="provider-link" type="button" onClick={onOpen}>
        {provider.name} 사용량 페이지 열기
        <span aria-hidden="true">↗</span>
      </button>
    </article>
  );
}

type HistoryMode = "projects" | "models";
type HistoryScope = "all" | "codex" | "claude";

function UsageHistory({
  providers,
  krwPerUsd,
}: {
  providers: ProviderSnapshot[];
  krwPerUsd: number | null;
}) {
  const [mode, setMode] = useState<HistoryMode>("projects");
  const [scope, setScope] = useState<HistoryScope>("all");

  const selectedProviders = providers.filter(
    (provider) => scope === "all" || provider.id === scope,
  );
  const entries = selectedProviders
    .flatMap((provider) =>
      (mode === "projects" ? provider.cost?.projects ?? [] : provider.cost?.models ?? []).map(
        (entry) => ({ ...entry, providerId: provider.id }),
      ),
    )
    .sort((left, right) => right.tokens - left.tokens)
    .slice(0, 12);
  const historyDays = Math.max(
    0,
    ...selectedProviders.map((provider) => provider.cost?.historyDays ?? 0),
  );
  const claudeProjectLabelsUnavailable =
    mode === "projects"
    && scope !== "codex"
    && providers.some(
      (provider) => provider.id === "claude" && (provider.cost?.projects.length ?? 0) === 0,
    );

  return (
    <details className="history-panel">
      <summary>
        <div className="history-summary-icon" aria-hidden="true">⌁</div>
        <div>
          <strong>사용량 기록</strong>
          <span>어떤 작업과 모델에서 토큰을 썼는지 확인</span>
        </div>
        <span className="history-period">최근 {historyDays || 30}일</span>
        <span className="history-chevron" aria-hidden="true">⌄</span>
      </summary>

      <div className="history-content">
        <div className="history-controls" aria-label="사용량 기록 필터">
          <div className="segmented-control">
            {(["all", "codex", "claude"] as const).map((value) => (
              <button
                key={value}
                type="button"
                className={scope === value ? "selected" : ""}
                onClick={() => setScope(value)}
              >
                {value === "all" ? "전체" : value === "codex" ? "Codex" : "Claude"}
              </button>
            ))}
          </div>
          <div className="segmented-control">
            <button
              type="button"
              className={mode === "projects" ? "selected" : ""}
              onClick={() => setMode("projects")}
            >
              작업별
            </button>
            <button
              type="button"
              className={mode === "models" ? "selected" : ""}
              onClick={() => setMode("models")}
            >
              모델별
            </button>
          </div>
        </div>

        {entries.length > 0 ? (
          <ol className="history-list">
            {entries.map((entry, index) => {
              const krw = entry.estimatedCostUsd != null && krwPerUsd != null
                ? entry.estimatedCostUsd * krwPerUsd
                : null;
              return (
                <li key={`${entry.providerId}-${entry.label}`}>
                  <span className="history-rank">{index + 1}</span>
                  <img src={PROVIDER_LOGOS[entry.providerId]} alt="" aria-hidden="true" />
                  <div className="history-entry-name">
                    <strong>{entry.label}</strong>
                    <span>{entry.providerId === "codex" ? "Codex" : "Claude"}</span>
                  </div>
                  <div className="history-entry-value">
                    <strong>{formatTokens(entry.tokens)} 토큰</strong>
                    <span>{krw == null ? "환산 준비 중" : `${formatKrw(krw)} 환산`}</span>
                  </div>
                </li>
              );
            })}
          </ol>
        ) : (
          <div className="history-empty">
            아직 표시할 {mode === "projects" ? "작업" : "모델"} 기록이 없습니다.
          </div>
        )}

        {claudeProjectLabelsUnavailable && (
          <p className="history-caveat">
            Claude는 현재 작업 이름을 제공하지 않아 모델별 기록만 확인할 수 있습니다.
          </p>
        )}
        <p className="history-privacy">
          작업 이름과 모델 기록은 이 맥의 로컬 로그에서만 읽으며 외부로 전송하지 않습니다.
        </p>
      </div>
    </details>
  );
}

function ResetDialog({
  credits,
  busy,
  onCancel,
  onConfirm,
}: {
  credits: ResetCredits;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const [acknowledged, setAcknowledged] = useState(false);

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="reset-dialog" role="dialog" aria-modal="true" aria-labelledby="reset-title">
        <div className="dialog-icon" aria-hidden="true">↻</div>
        <p className="dialog-kicker">되돌릴 수 없는 작업</p>
        <h2 id="reset-title">Codex 한도를 지금 초기화할까요?</h2>
        <p className="dialog-copy">
          보유한 초기화권 {credits.availableCount}장 중 1장을 사용합니다. Claude 한도에는 영향을 주지 않습니다.
        </p>
        <div className="dialog-fact">
          <span>가장 가까운 만료</span>
          <strong>{formatAbsoluteTime(credits.nearestExpiry)}</strong>
        </div>
        <label className="confirm-check">
          <input
            type="checkbox"
            checked={acknowledged}
            disabled={busy}
            onChange={(event) => setAcknowledged(event.currentTarget.checked)}
          />
          <span>초기화권 1장이 즉시 사용되는 것을 이해했습니다.</span>
        </label>
        <div className="dialog-actions">
          <button className="secondary-button" type="button" onClick={onCancel} disabled={busy}>
            취소
          </button>
          <button
            className="destructive-button"
            type="button"
            onClick={onConfirm}
            disabled={!acknowledged || busy}
          >
            {busy ? "공식 응답 확인 중…" : "초기화권 1장 사용"}
          </button>
        </div>
      </section>
    </div>
  );
}

function App() {
  const [snapshot, setSnapshot] = useState<AppSnapshot>(EMPTY_SNAPSHOT);
  const [refreshing, setRefreshing] = useState(false);
  const [nowMs, setNowMs] = useState(Date.now());
  const [autostart, setAutostart] = useState(false);
  const [notificationState, setNotificationState] = useState<NotificationState>("checking");
  const [resetCredits, setResetCredits] = useState<ResetCredits | null>(null);
  const [resetting, setResetting] = useState(false);
  const [toast, setToast] = useState<string | null>(null);

  const providers = useMemo(
    () =>
      (["codex", "claude"] as const).map(
        (id) => snapshot.providers.find((provider) => provider.id === id)
          ?? EMPTY_SNAPSHOT.providers.find((provider) => provider.id === id)!,
      ),
    [snapshot.providers],
  );

  useEffect(() => {
    const clock = window.setInterval(() => setNowMs(Date.now()), 1_000);
    return () => window.clearInterval(clock);
  }, []);

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];

    invoke<AppSnapshot>("get_snapshot")
      .then((value) => {
        if (!disposed) setSnapshot(value);
      })
      .catch((error) => {
        if (!disposed) setToast(String(error));
      });

    listen<AppSnapshot>("snapshot-updated", (event) => {
      if (!disposed) setSnapshot(event.payload);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else unlisteners.push(unlisten);
    });
    listen<boolean>("snapshot-refreshing", (event) => {
      if (!disposed) setRefreshing(event.payload);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else unlisteners.push(unlisten);
    });

    isAutostartEnabled()
      .then((enabled) => {
        if (!disposed) setAutostart(enabled);
      })
      .catch(() => undefined);
    isPermissionGranted()
      .then((granted) => {
        if (!disposed) setNotificationState(granted ? "granted" : "not-granted");
      })
      .catch(() => {
        if (!disposed) setNotificationState("not-granted");
      });

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    if (!toast) return undefined;
    const timer = window.setTimeout(() => setToast(null), 6_000);
    return () => window.clearTimeout(timer);
  }, [toast]);

  async function refresh() {
    if (refreshing) return;
    setRefreshing(true);
    try {
      setSnapshot(await invoke<AppSnapshot>("refresh_snapshot"));
    } catch (error) {
      setToast(String(error));
    } finally {
      setRefreshing(false);
    }
  }

  async function toggleAutostart() {
    try {
      if (autostart) await disableAutostart();
      else await enableAutostart();
      setAutostart(!autostart);
    } catch (error) {
      setToast(`자동 실행 설정을 바꾸지 못했습니다: ${String(error)}`);
    }
  }

  async function enableNotifications() {
    try {
      const permission = await requestPermission();
      setNotificationState(permission === "granted" ? "granted" : "not-granted");
      if (permission !== "granted") {
        setToast("알림이 허용되지 않았습니다. macOS 시스템 설정에서 바꿀 수 있어요.");
      }
    } catch (error) {
      setToast(`알림 권한을 확인하지 못했습니다: ${String(error)}`);
    }
  }

  async function confirmReset() {
    setResetting(true);
    try {
      const result = await invoke<ResetResult>("consume_codex_reset_credit", {
        confirmation: "RESET_ONE_CREDIT",
      });
      setToast(result.message);
      setResetCredits(null);
    } catch (error) {
      setToast(`초기화하지 못했습니다: ${String(error)}`);
    } finally {
      setResetting(false);
    }
  }

  return (
    <main className="app-shell">
      <header className="app-header">
        <div className="brand-lockup">
          <div className="rice-mark" aria-hidden="true">米</div>
          <div>
            <p className="eyebrow">LOCAL USAGE MONITOR</p>
            <h1>쌀먹</h1>
          </div>
        </div>
        <button
          className={`refresh-button ${refreshing ? "is-refreshing" : ""}`}
          type="button"
          onClick={refresh}
          disabled={refreshing}
          aria-label="지금 사용량 갱신"
        >
          <span aria-hidden="true">↻</span>
          {refreshing ? "읽는 중" : "지금 갱신"}
        </button>
      </header>

      <section className="privacy-strip" aria-label="갱신 상태">
        <span className="privacy-dot" />
        <strong>이 맥에서만 읽음</strong>
        <span>·</span>
        <span>1분 자동 갱신</span>
        <span>·</span>
        <span>마지막 시도 {formatRelativeEpoch(snapshot.lastAttemptAtEpochSeconds, nowMs)}</span>
      </section>

      {snapshot.globalError && (
        <div className="global-error" role="alert">
          <strong>지금은 새 값을 읽지 못했습니다.</strong>
          <span>{snapshot.globalError}</span>
        </div>
      )}

      <section className="provider-list" aria-label="AI 사용량">
        {providers.map((provider) => (
          <ProviderCard
            key={provider.id}
            provider={provider}
            nowMs={nowMs}
            onOpen={() => openUrl(PROVIDER_URLS[provider.id])}
            onReset={(credits) => setResetCredits(credits)}
          />
        ))}
      </section>

      <UsageHistory
        providers={providers}
        krwPerUsd={snapshot.exchangeRate?.krwPerUsd ?? null}
      />

      <section className="preferences" aria-labelledby="preferences-title">
        <div className="preferences-heading">
          <div>
            <p className="eyebrow">QUIET BY DEFAULT</p>
            <h2 id="preferences-title">상시 확인 설정</h2>
          </div>
          <span>설정은 이 맥에만 저장됩니다.</span>
        </div>

        <div className="preference-row">
          <div>
            <strong>로그인하면 자동 실행</strong>
            <p>창은 띄우지 않고 메뉴 막대에서 시작합니다.</p>
          </div>
          <button
            className={`switch ${autostart ? "switch-on" : ""}`}
            type="button"
            role="switch"
            aria-checked={autostart}
            aria-label="로그인 시 자동 실행"
            onClick={toggleAutostart}
          >
            <span />
          </button>
        </div>

        <div className="preference-row">
          <div>
            <strong>중요할 때만 알림</strong>
            <p>남은 양 20%·5%, 소진, 초기화, 연결 오류를 알립니다.</p>
          </div>
          {notificationState === "granted" ? (
            <span className="permission-ok">켜짐</span>
          ) : (
            <button className="small-button" type="button" onClick={enableNotifications}>
              알림 켜기
            </button>
          )}
        </div>
      </section>

      <footer className="app-footer">
        <div>
          <strong>쌀먹 0.1.0</strong>
          <span>CodexBar {snapshot.sidecarVersion} 기반</span>
        </div>
        <button type="button" onClick={() => invoke("quit_app")}>완전히 종료</button>
      </footer>

      {resetCredits && (
        <ResetDialog
          credits={resetCredits}
          busy={resetting}
          onCancel={() => !resetting && setResetCredits(null)}
          onConfirm={confirmReset}
        />
      )}

      {toast && (
        <div className="toast" role="status">
          {toast}
        </div>
      )}
    </main>
  );
}

export default App;
