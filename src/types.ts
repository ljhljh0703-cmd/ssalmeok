export type UsageWindow = {
  id: string;
  label: string;
  usedPercent: number;
  remainingPercent: number;
  resetsAt: string | null;
  windowMinutes: number | null;
};

export type UsageBreakdown = {
  label: string;
  tokens: number;
  estimatedCostUsd: number | null;
};

export type CostSummary = {
  recentDate: string | null;
  recentTokens: number | null;
  recentCostUsd: number | null;
  last30DaysTokens: number | null;
  last30DaysCostUsd: number | null;
  estimated: boolean;
  historyDays: number | null;
  projects: UsageBreakdown[];
  models: UsageBreakdown[];
};

export type ResetCredits = {
  availableCount: number;
  nearestExpiry: string | null;
  title: string | null;
};

export type ProviderSnapshot = {
  id: "codex" | "claude";
  name: string;
  source: string;
  windows: UsageWindow[];
  cost: CostSummary | null;
  resetCredits: ResetCredits | null;
  lastSuccessAtEpochSeconds: number;
  stale: boolean;
  error: string | null;
};

export type ExchangeRate = {
  krwPerUsd: number;
  referenceDate: string;
  fetchedAtEpochSeconds: number;
  source: string;
};

export type AppSnapshot = {
  generatedAtEpochSeconds: number;
  lastAttemptAtEpochSeconds: number;
  lastCostScanAtEpochSeconds: number;
  staleAfterSeconds: number;
  refreshIntervalSeconds: number;
  sidecarVersion: string;
  providers: ProviderSnapshot[];
  exchangeRate: ExchangeRate | null;
  globalError: string | null;
};

export type ResetResult = {
  outcome: "reset" | "alreadyRedeemed" | "nothingToReset" | "noCredit" | "unknown";
  message: string;
  reusedPendingAttempt: boolean;
};
