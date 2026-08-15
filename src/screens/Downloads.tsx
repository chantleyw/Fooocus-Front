import { Check, Download, Pause, Play, Trash2, X } from "lucide-react";

import { api, type Job } from "../lib/api";
import { formatBytes, formatEta, formatSpeed } from "../lib/format";
import { useStore } from "../store";
import { Banner, Chip, EmptyState, ProgressBar, ScreenHeader } from "../components/ui";

const STATE_TONE = {
  queued: "default",
  downloading: "accent",
  paused: "warning",
  completed: "success",
  failed: "danger",
  cancelled: "default",
} as const;

const STATE_LABEL: Record<Job["state"], string> = {
  queued: "Queued",
  downloading: "Downloading",
  paused: "Paused",
  completed: "Complete",
  failed: "Failed",
  cancelled: "Cancelled",
};

export function Downloads() {
  const { jobs, setScreen } = useStore();

  const active = jobs.filter((job) => job.state === "downloading" || job.state === "queued");
  const finished = jobs.filter((job) =>
    ["completed", "failed", "cancelled"].includes(job.state),
  );
  const totalSpeed = active.reduce((sum, job) => sum + job.speed, 0);

  return (
    <div className="screen">
      <ScreenHeader
        title="Downloads"
        subtitle={
          active.length
            ? `${active.length} in progress · ${formatSpeed(totalSpeed)}`
            : "Model downloads, with resume support"
        }
        actions={
          finished.length > 0 ? (
            <button
              className="btn"
              onClick={() => {
                void api.clearFinishedDownloads();
                useStore.setState((state) => ({
                  jobs: state.jobs.filter(
                    (job) => !["completed", "failed", "cancelled"].includes(job.state),
                  ),
                }));
              }}
            >
              <Trash2 size={15} />
              Clear finished
            </button>
          ) : undefined
        }
      />

      <div className="screen-body">
        {jobs.length === 0 ? (
          <EmptyState
            icon={<Download size={22} />}
            title="No downloads yet"
            action={
              <button className="btn btn-primary" onClick={() => setScreen("models")}>
                Browse available models
              </button>
            }
          >
            Anything you fetch from the Models tab appears here with live progress. Downloads resume
            where they left off if interrupted.
          </EmptyState>
        ) : (
          <>
            {jobs.map((job) => {
              const fraction = job.total ? job.downloaded / job.total : 0;
              const busy = job.state === "downloading";

              return (
                <div className="job" key={job.id}>
                  <div className="job-main">
                    <div className="job-head">
                      <span className="job-name truncate">{job.name}</span>
                      <Chip tone={STATE_TONE[job.state]}>{STATE_LABEL[job.state]}</Chip>
                      <span
                        className="mono truncate"
                        style={{ color: "var(--text-faint)", fontSize: 11 }}
                      >
                        {job.filename}
                      </span>
                    </div>

                    <ProgressBar
                      value={job.state === "completed" ? 1 : fraction}
                      indeterminate={busy && !job.total}
                    />

                    <div className="job-stats">
                      <span>
                        {formatBytes(job.downloaded)}
                        {job.total ? ` of ${formatBytes(job.total)}` : ""}
                        {job.total ? ` · ${Math.round(fraction * 100)}%` : ""}
                      </span>
                      {busy && (
                        <>
                          <span>{formatSpeed(job.speed)}</span>
                          <span>{formatEta(job.downloaded, job.total, job.speed)} left</span>
                        </>
                      )}
                    </div>

                    {job.error && (
                      <div style={{ marginTop: 8 }}>
                        <Banner tone="danger">{job.error}</Banner>
                      </div>
                    )}
                  </div>

                  <div className="job-actions">
                    {busy && (
                      <button
                        className="btn btn-ghost btn-icon"
                        title="Pause"
                        onClick={() => void api.pauseDownload(job.id)}
                      >
                        <Pause size={15} />
                      </button>
                    )}
                    {(job.state === "paused" || job.state === "failed") && (
                      <button
                        className="btn btn-ghost btn-icon"
                        title="Resume"
                        onClick={() => void api.startDownload(job.id)}
                      >
                        <Play size={15} />
                      </button>
                    )}
                    {job.state !== "completed" && job.state !== "cancelled" && (
                      <button
                        className="btn btn-ghost btn-icon"
                        title="Cancel and discard"
                        onClick={() => void api.cancelDownload(job.id)}
                      >
                        <X size={15} />
                      </button>
                    )}
                    {job.state === "completed" && (
                      <span style={{ color: "var(--success)", padding: 7 }}>
                        <Check size={16} />
                      </span>
                    )}
                  </div>
                </div>
              );
            })}
          </>
        )}
      </div>
    </div>
  );
}
