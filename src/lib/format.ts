export function clampPercent(value: number): number {
  return Math.min(100, Math.max(0, value));
}

export function formatPercent(value: number): string {
  const clamped = clampPercent(value);
  return `${clamped >= 10 ? Math.round(clamped) : clamped.toFixed(1)}%`;
}

export function formatTokens(value: number | null): string {
  if (value == null) return "—";
  return new Intl.NumberFormat("ko-KR", {
    notation: "compact",
    maximumFractionDigits: 1,
  }).format(value);
}

export function formatUsd(value: number | null): string {
  if (value == null) return "—";
  return new Intl.NumberFormat("ko-KR", {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: value >= 100 ? 0 : 2,
    maximumFractionDigits: value >= 100 ? 0 : 2,
  }).format(value);
}

export function formatKrw(value: number | null): string {
  if (value == null || !Number.isFinite(value)) return "—";
  return new Intl.NumberFormat("ko-KR", {
    style: "currency",
    currency: "KRW",
    maximumFractionDigits: 0,
  }).format(value);
}

export function formatAbsoluteTime(value: string | null): string {
  if (!value) return "시각 미정";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "시각 미정";
  return new Intl.DateTimeFormat("ko-KR", {
    month: "numeric",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(date);
}

export function formatCountdown(value: string | null, nowMs: number): string {
  if (!value) return "초기화 시각 미정";
  const resetMs = new Date(value).getTime();
  if (Number.isNaN(resetMs)) return "초기화 시각 미정";
  const difference = resetMs - nowMs;
  if (difference <= 0) return "초기화 확인 중";

  const totalMinutes = Math.ceil(difference / 60_000);
  const days = Math.floor(totalMinutes / 1_440);
  const hours = Math.floor((totalMinutes % 1_440) / 60);
  const minutes = totalMinutes % 60;

  if (days > 0) return `${days}일 ${hours}시간 후`;
  if (hours > 0) return `${hours}시간 ${minutes}분 후`;
  return `${minutes}분 후`;
}

export function formatRelativeEpoch(epochSeconds: number, nowMs: number): string {
  if (!epochSeconds) return "아직 없음";
  const elapsed = Math.max(0, Math.floor((nowMs - epochSeconds * 1_000) / 1_000));
  if (elapsed < 10) return "방금 전";
  if (elapsed < 60) return `${elapsed}초 전`;
  const minutes = Math.floor(elapsed / 60);
  if (minutes < 60) return `${minutes}분 전`;
  return `${Math.floor(minutes / 60)}시간 전`;
}

export function sourceLabel(source: string): string {
  switch (source) {
    case "oauth":
      return "기존 로그인";
    case "claude":
      return "Claude 로컬";
    case "local":
      return "로컬 기록";
    case "web":
    case "openai-web":
      return "웹 계정";
    default:
      return "로컬 연결";
  }
}

export function riskTone(remainingPercent: number): "safe" | "watch" | "critical" {
  if (remainingPercent <= 5) return "critical";
  if (remainingPercent <= 20) return "watch";
  return "safe";
}
