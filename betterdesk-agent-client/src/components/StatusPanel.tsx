import { Component, Show, createSignal, onMount, onCleanup } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { t } from "../lib/i18n";
import { frontendLog } from "../lib/logger";

interface AgentStatus {
  registered: boolean;
  connected: boolean;
  server_address: string;
  device_id: string;
  hostname: string;
  platform: string;
  version: string;
  uptime_secs: number;
  last_sync: string;
}

interface SidecarStatus {
  running: boolean;
  pid: number;
  restart_count: number;
  state: string;
  binary_path: string;
  cdap_url: string;
}

const StatusPanel: Component = () => {
  const [status, setStatus] = createSignal<AgentStatus | null>(null);
  const [sidecar, setSidecar] = createSignal<SidecarStatus | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [copyFeedback, setCopyFeedback] = createSignal(false);
  const [diagFeedback, setDiagFeedback] = createSignal<"" | "ok" | "error">("");
  const [sidecarAction, setSidecarAction] = createSignal<"" | "busy">(""); 
  const [sidecarError, setSidecarError] = createSignal("");
  let initialSnapshotLogged = false;

  let pollInterval: ReturnType<typeof setInterval>;

  const fetchStatus = async () => {
    try {
      const s = await invoke<AgentStatus>("get_agent_status");
      setStatus(s);
    } catch (error) {
      frontendLog("warn", "status", "get_agent_status failed", error);
      // Keep last known status
    }
    try {
      const sc = await invoke<SidecarStatus>("get_sidecar_status");
      setSidecar(sc);
    } catch (error) {
      frontendLog("warn", "status", "get_sidecar_status failed", error);
      // Not critical
    }

    if (!initialSnapshotLogged && (status() || sidecar())) {
      frontendLog("info", "status", "Initial status snapshot loaded", {
        registered: status()?.registered ?? false,
        connected: status()?.connected ?? false,
        sidecarState: sidecar()?.state ?? "unknown",
        sidecarRunning: sidecar()?.running ?? false,
      });
      initialSnapshotLogged = true;
    }

    setLoading(false);
  };

  onMount(() => {
    frontendLog("debug", "status", "Status panel mounted");
    fetchStatus();
    pollInterval = setInterval(fetchStatus, 5000);
  });

  onCleanup(() => clearInterval(pollInterval));

  const copyId = async () => {
    const s = status();
    if (!s) return;
    try {
      await invoke("copy_to_clipboard", { text: s.device_id });
      setCopyFeedback(true);
      setTimeout(() => setCopyFeedback(false), 2000);
    } catch {}
  };

  const reconnect = async () => {
    try {
      frontendLog("info", "status", "Manual reconnect requested");
      await invoke("reconnect_agent");
      await fetchStatus();
    } catch (error) {
      frontendLog("error", "status", "reconnect_agent failed", error);
    }
  };

  const startSidecar = async () => {
    setSidecarAction("busy");
    setSidecarError("");
    try {
      frontendLog("info", "status.sidecar", "Starting CDAP sidecar");
      await invoke("start_sidecar");
      setTimeout(fetchStatus, 1000);
    } catch (error) {
      frontendLog("error", "status.sidecar", "start_sidecar failed", error);
      setSidecarError(String(error));
    }
    setSidecarAction("");
  };

  const stopSidecar = async () => {
    setSidecarAction("busy");
    setSidecarError("");
    try {
      frontendLog("info", "status.sidecar", "Stopping CDAP sidecar");
      await invoke("stop_sidecar");
      setTimeout(fetchStatus, 500);
    } catch (error) {
      frontendLog("error", "status.sidecar", "stop_sidecar failed", error);
    }
    setSidecarAction("");
  };

  const restartSidecar = async () => {
    setSidecarAction("busy");
    setSidecarError("");
    try {
      frontendLog("info", "status.sidecar", "Restarting CDAP sidecar");
      await invoke("restart_sidecar");
      setTimeout(fetchStatus, 1000);
    } catch (error) {
      frontendLog("error", "status.sidecar", "restart_sidecar failed", error);
      setSidecarError(String(error));
    }
    setSidecarAction("");
  };

  const sendDiagnostics = async () => {
    try {
      frontendLog("info", "status", "Diagnostics upload requested");
      await invoke("send_diagnostics");
      setDiagFeedback("ok");
      setTimeout(() => setDiagFeedback(""), 3000);
    } catch (error) {
      frontendLog("error", "status", "send_diagnostics failed", error);
      setDiagFeedback("error");
      setTimeout(() => setDiagFeedback(""), 3000);
    }
  };

  const formatUptime = (secs: number): string => {
    if (secs < 60) return `${secs}s`;
    if (secs < 3600) return `${Math.floor(secs / 60)}m`;
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    return `${h}h ${m}m`;
  };

  return (
    <div class="page-content">
      <h2 class="page-title">{t("status.title")}</h2>

      {loading() ? (
        <div class="loading-state">
          <span class="material-symbols-rounded spin">sync</span>
          <span>{t("common.loading")}</span>
        </div>
      ) : (
        <>
          <div class="status-hero">
            <div class={`status-indicator ${status()?.connected ? "online" : "offline"}`}>
              <span class="material-symbols-rounded">
                {status()?.connected ? "cloud_done" : "cloud_off"}
              </span>
              <span class="status-text">
                {status()?.connected ? t("status.connected") : t("status.disconnected")}
              </span>
            </div>
          </div>

          <div class="info-grid">
            <div class="info-card">
              <div class="info-label">{t("status.device_id")}</div>
              <div class="info-value id-row">
                <code>{status()?.device_id || "—"}</code>
                <button class="icon-btn" onClick={copyId} title={t("status.copy_id")}>
                  <span class="material-symbols-rounded">
                    {copyFeedback() ? "check" : "content_copy"}
                  </span>
                </button>
                {copyFeedback() && <span class="form-success">{t("status.id_copied")}</span>}
              </div>
            </div>

            <div class="info-card">
              <div class="info-label">{t("status.server")}</div>
              <div class="info-value">{status()?.server_address || "—"}</div>
            </div>

            <div class="info-card">
              <div class="info-label">{t("status.hostname")}</div>
              <div class="info-value">{status()?.hostname || "—"}</div>
            </div>

            <div class="info-card">
              <div class="info-label">{t("status.platform")}</div>
              <div class="info-value">{status()?.platform || "—"}</div>
            </div>

            <div class="info-card">
              <div class="info-label">{t("status.version")}</div>
              <div class="info-value">{status()?.version || "—"}</div>
            </div>

            <div class="info-card">
              <div class="info-label">{t("status.uptime")}</div>
              <div class="info-value">
                {status()?.uptime_secs ? formatUptime(status()!.uptime_secs) : "—"}
              </div>
            </div>

            <div class="info-card">
              <div class="info-label">{t("status.last_sync")}</div>
              <div class="info-value">{status()?.last_sync || "—"}</div>
            </div>
          </div>

          {/* ── CDAP Sidecar Status ── */}
          <div class="section-header">
            <span class="material-symbols-rounded">settings_suggest</span>
            {t("status.sidecar_title")}
          </div>
          <div class="sidecar-card">
            <div class="sidecar-status-row">
              <span class={`status-dot ${sidecar()?.running ? "dot-green" : "dot-red"}`} />
              <span class="sidecar-state">
                {sidecar()?.running
                  ? t("status.sidecar_running")
                  : sidecar()?.state === "not_configured"
                  ? t("status.sidecar_not_configured")
                  : t("status.sidecar_stopped")}
              </span>
              {sidecar()?.pid ? (
                <span class="sidecar-pid">PID {sidecar()!.pid}</span>
              ) : null}
              {(sidecar()?.restart_count ?? 0) > 0 ? (
                <span class="sidecar-restarts" title={t("status.sidecar_restarts_hint")}>
                  <span class="material-symbols-rounded" style="font-size:14px">refresh</span>
                  {sidecar()!.restart_count}
                </span>
              ) : null}
            </div>

            {sidecar()?.cdap_url && (
              <div class="sidecar-detail">
                <span class="material-symbols-rounded">hub</span>
                <code>{sidecar()!.cdap_url}</code>
              </div>
            )}

            {sidecar()?.binary_path && (
              <div class="sidecar-detail sidecar-path">
                <span class="material-symbols-rounded">terminal</span>
                <span title={sidecar()!.binary_path}>
                  {sidecar()!.binary_path.split(/[/\\]/).pop()}
                </span>
              </div>
            )}

            <div class="sidecar-actions">
              {!sidecar()?.running ? (
                <button
                  class="btn btn-primary btn-sm"
                  onClick={startSidecar}
                  disabled={sidecarAction() === "busy"}
                >
                  <span class="material-symbols-rounded">play_arrow</span>
                  {t("status.sidecar_start")}
                </button>
              ) : (
                <>
                  <button
                    class="btn btn-secondary btn-sm"
                    onClick={restartSidecar}
                    disabled={sidecarAction() === "busy"}
                  >
                    <span class="material-symbols-rounded">refresh</span>
                    {t("status.sidecar_restart")}
                  </button>
                  <button
                    class="btn btn-danger btn-sm"
                    onClick={stopSidecar}
                    disabled={sidecarAction() === "busy"}
                  >
                    <span class="material-symbols-rounded">stop</span>
                    {t("status.sidecar_stop")}
                  </button>
                </>
              )}
            </div>

            <Show when={sidecarError()}>
              <div class="form-error">{sidecarError()}</div>
            </Show>
          </div>

          <div class="status-actions">
            {!status()?.connected && (
              <button class="btn btn-primary" onClick={reconnect}>
                <span class="material-symbols-rounded">refresh</span>
                {t("status.reconnect")}
              </button>
            )}
            <button class="btn btn-secondary" onClick={sendDiagnostics}>
              <span class="material-symbols-rounded">
                {diagFeedback() === "ok" ? "check" : diagFeedback() === "error" ? "error" : "bug_report"}
              </span>
              {diagFeedback() === "ok"
                ? t("status.diagnostics_sent")
                : diagFeedback() === "error"
                ? t("status.diagnostics_error")
                : t("status.send_diagnostics")}
            </button>
          </div>
        </>
      )}
    </div>
  );
};

export default StatusPanel;
