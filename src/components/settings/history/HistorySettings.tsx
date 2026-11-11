import React, {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { readFile } from "@tauri-apps/plugin-fs";
import {
  AudioLines,
  Check,
  Copy,
  FileAudio,
  FilePlus,
  FolderOpen,
  RotateCcw,
  Star,
  Trash2,
  X,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { notify } from "@/stores/noticeStore";
import {
  commands,
  events,
  type HistoryEntry,
  type HistoryUpdatePayload,
} from "@/bindings";
import { useOsType } from "@/hooks/useOsType";
import {
  useTranscriptionJobsStore,
  type ActiveJobStatus,
} from "@/stores/transcriptionJobsStore";
import { formatDateTime } from "@/utils/dateFormat";
import { AudioPlayer, AudioPlayerGroup } from "../../ui/AudioPlayer";
import { Button } from "../../ui/Button";
import { copyToClipboard } from "./clipboard";
import {
  pickAndTranscribeFiles,
  useFileReadsStore,
  type FileRead,
} from "./transcribeFiles";

const IconButton: React.FC<{
  onClick: () => void;
  title: string;
  disabled?: boolean;
  active?: boolean;
  children: React.ReactNode;
}> = ({ onClick, title, disabled, active, children }) => (
  <button
    type="button"
    onClick={onClick}
    disabled={disabled}
    title={title}
    aria-label={title}
    className={`vt-icon-btn${active ? " vt-icon-btn--active" : ""}`}
  >
    {children}
  </button>
);

const PAGE_SIZE = 30;

// Long text folds to this many lines until "Show more".
const FOLDED_LINES = 4;

// A dropped or picked file while its audio is read, before it's an entry.
const FileReadRow: React.FC<{ read: FileRead }> = ({ read }) => {
  const { t } = useTranslation();
  return (
    <div className="vt-history__item">
      <div className="vt-history__head">
        <div className="vt-history__meta">
          <FileAudio
            width={13}
            height={13}
            className="vt-history__source-icon"
          />
          <span className="vt-history__source-name" title={read.name}>
            {read.name}
          </span>
        </div>
      </div>
      <div className="vt-history__body">
        <p className="vt-history__text vt-history__text--pending">
          {t("settings.history.readingFile")}
        </p>
      </div>
    </div>
  );
};

interface HeaderButtonProps {
  onClick: () => void;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
}

const HeaderButton: React.FC<HeaderButtonProps> = ({
  onClick,
  label,
  icon: Icon,
}) => (
  <Button onClick={onClick} variant="secondary" size="sm" title={label}>
    <Icon className="vt-btn__icon" />
    <span>{label}</span>
  </Button>
);

export const HistorySettings: React.FC = () => {
  const { t } = useTranslation();
  const osType = useOsType();
  const jobs = useTranscriptionJobsStore((state) => state.jobs);
  const reads = useFileReadsStore((state) => state.reads);
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [hasMore, setHasMore] = useState(true);
  const sentinelRef = useRef<HTMLDivElement>(null);
  const entriesRef = useRef<HistoryEntry[]>([]);
  const loadingRef = useRef(false);

  // Keep ref in sync for use in IntersectionObserver callback
  useEffect(() => {
    entriesRef.current = entries;
  }, [entries]);

  const loadPage = useCallback(async (cursor?: number) => {
    const isFirstPage = cursor === undefined;
    if (!isFirstPage && loadingRef.current) return;
    loadingRef.current = true;

    if (isFirstPage) setLoading(true);

    try {
      const result = await commands.getHistoryEntries(
        cursor ?? null,
        PAGE_SIZE,
      );
      if (result.status === "ok") {
        const { entries: newEntries, has_more } = result.data;
        setEntries((prev) => {
          if (isFirstPage) return newEntries;
          const existingIds = new Set(prev.map((entry) => entry.id));
          return [
            ...prev,
            ...newEntries.filter((entry) => !existingIds.has(entry.id)),
          ];
        });
        setHasMore(has_more);
      }
    } catch (error) {
      console.error("Failed to load history entries:", error);
    } finally {
      setLoading(false);
      loadingRef.current = false;
    }
  }, []);

  // Initial load
  useEffect(() => {
    loadPage();
  }, [loadPage]);

  // Infinite scroll via IntersectionObserver
  useEffect(() => {
    if (loading) return;

    const sentinel = sentinelRef.current;
    if (!sentinel || !hasMore) return;

    const observer = new IntersectionObserver(
      (observerEntries) => {
        const first = observerEntries[0];
        if (first.isIntersecting) {
          const lastEntry = entriesRef.current[entriesRef.current.length - 1];
          if (lastEntry) {
            loadPage(lastEntry.id);
          }
        }
      },
      { threshold: 0 },
    );

    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [loading, hasMore, loadPage]);

  // Listen for new entries added from the transcription pipeline
  useEffect(() => {
    const unlisten = events.historyUpdatePayload.listen((event) => {
      const payload: HistoryUpdatePayload = event.payload;
      if (payload.action === "added") {
        setEntries((prev) => [
          payload.entry,
          ...prev.filter((entry) => entry.id !== payload.entry.id),
        ]);
      } else if (payload.action === "updated") {
        setEntries((prev) =>
          prev.map((e) => (e.id === payload.entry.id ? payload.entry : e)),
        );
      }
      // "deleted" and "toggled" are handled by optimistic updates only,
      // so we intentionally ignore them here to avoid double-mutation.
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const toggleSaved = async (id: number) => {
    // Optimistic update
    setEntries((prev) =>
      prev.map((e) => (e.id === id ? { ...e, saved: !e.saved } : e)),
    );
    try {
      const result = await commands.toggleHistoryEntrySaved(id);
      if (result.status !== "ok") {
        // Revert on failure
        setEntries((prev) =>
          prev.map((e) => (e.id === id ? { ...e, saved: !e.saved } : e)),
        );
      }
    } catch (error) {
      console.error("Failed to toggle saved status:", error);
      // Revert on failure
      setEntries((prev) =>
        prev.map((e) => (e.id === id ? { ...e, saved: !e.saved } : e)),
      );
    }
  };

  const getAudioUrl = useCallback(
    async (fileName: string) => {
      try {
        const result = await commands.getAudioFilePath(fileName);
        if (result.status === "ok") {
          if (osType === "linux") {
            const fileData = await readFile(result.data);
            const blob = new Blob([fileData], { type: "audio/wav" });
            return URL.createObjectURL(blob);
          }
          return convertFileSrc(result.data, "asset");
        }
        return null;
      } catch (error) {
        console.error("Failed to get audio file path:", error);
        return null;
      }
    },
    [osType],
  );

  const deleteAudioEntry = async (id: number) => {
    // Optimistically remove
    setEntries((prev) => prev.filter((e) => e.id !== id));
    try {
      const result = await commands.deleteHistoryEntry(id);
      if (result.status !== "ok") {
        throw new Error(String(result.error));
      }
    } catch (error) {
      console.error("Failed to delete entry:", error);
      loadPage();
      throw error;
    }
  };

  const transcribeFile = async () => {
    try {
      await pickAndTranscribeFiles();
    } catch (error) {
      console.error("Failed to pick a file to transcribe:", error);
      notify({ title: t("settings.history.transcribeFailed") });
    }
  };

  const openRecordingsFolder = async () => {
    try {
      const result = await commands.openRecordingsFolder();
      if (result.status !== "ok") {
        throw new Error(String(result.error));
      }
    } catch (error) {
      console.error("Failed to open recordings folder:", error);
    }
  };

  let content: React.ReactNode;

  if (loading) {
    content = (
      <div className="vt-history__state">
        <span className="vt-spinner" />
        <span>{t("settings.history.loading")}</span>
      </div>
    );
  } else if (entries.length === 0 && reads.length === 0) {
    content = (
      <div className="vt-history__empty">
        <span className="vt-history__empty-badge">
          <AudioLines width={22} height={22} />
        </span>
        <p className="vt-history__empty-text">{t("settings.history.empty")}</p>
        <Button
          onClick={transcribeFile}
          variant="secondary"
          size="sm"
          className="vt-history__empty-action"
        >
          <FilePlus className="vt-btn__icon" />
          <span>{t("settings.history.transcribeFile")}</span>
        </Button>
      </div>
    );
  } else {
    content = (
      <AudioPlayerGroup>
        <div className="vt-history">
          {reads.map((read) => (
            <FileReadRow key={`read-${read.id}`} read={read} />
          ))}
          {entries.map((entry) => (
            <HistoryEntryComponent
              key={entry.id}
              entry={entry}
              job={jobs[entry.id]}
              onToggleSaved={() => toggleSaved(entry.id)}
              onCopyText={() => copyToClipboard(entry.transcription_text)}
              getAudioUrl={getAudioUrl}
              deleteAudio={deleteAudioEntry}
              retryTranscription={commands.retryHistoryEntryTranscription}
              cancelTranscription={commands.cancelHistoryEntryTranscription}
            />
          ))}
          {/* Sentinel for infinite scroll */}
          <div ref={sentinelRef} className="vt-history__tail" />
        </div>
      </AudioPlayerGroup>
    );
  }

  return (
    <div className="vt-stack">
      <section className="vt-group">
        {/* The page header already says History, so the card carries only the
            two actions that belong to it. */}
        <div className="vt-group__head">
          <HeaderButton
            onClick={transcribeFile}
            label={t("settings.history.transcribeFile")}
            icon={FilePlus}
          />
          <HeaderButton
            onClick={openRecordingsFolder}
            label={t("settings.history.openFolder")}
            icon={FolderOpen}
          />
        </div>
        <div className="vt-group__card vt-group__card--clip">{content}</div>
      </section>
    </div>
  );
};

interface HistoryEntryProps {
  entry: HistoryEntry;
  job?: ActiveJobStatus;
  onToggleSaved: () => void;
  onCopyText: () => Promise<boolean>;
  getAudioUrl: (fileName: string) => Promise<string | null>;
  deleteAudio: (id: number) => Promise<void>;
  retryTranscription: (id: number) => Promise<void>;
  cancelTranscription: (id: number) => Promise<void>;
}

const HistoryEntryComponent: React.FC<HistoryEntryProps> = ({
  entry,
  job,
  onToggleSaved,
  onCopyText,
  getAudioUrl,
  deleteAudio,
  retryTranscription,
  cancelTranscription,
}) => {
  const { t, i18n } = useTranslation();
  const [showCopied, setShowCopied] = useState(false);
  // Pressed cancel: a running job stops after its current chunk.
  const [cancelling, setCancelling] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [overflows, setOverflows] = useState(false);
  const textRef = useRef<HTMLParagraphElement>(null);

  // Queued or being transcribed in the background.
  const retrying = job !== undefined;
  useEffect(() => {
    if (!retrying) setCancelling(false);
  }, [retrying]);
  const hasTranscription = entry.transcription_text.trim().length > 0;
  const folded = hasTranscription && !retrying && !expanded;

  // Offer "Show more" only when the folded text is actually cut off.
  useLayoutEffect(() => {
    const el = textRef.current;
    if (!el || !folded) return;
    const measure = () => setOverflows(el.scrollHeight > el.clientHeight + 1);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [folded, entry.transcription_text]);

  const handleLoadAudio = useCallback(
    () => getAudioUrl(entry.file_name),
    [getAudioUrl, entry.file_name],
  );

  const handleCopyText = async () => {
    if (!hasTranscription) {
      return;
    }

    const copied = await onCopyText();
    if (!copied) {
      notify({ title: t("settings.history.copyError") });
      return;
    }

    setShowCopied(true);
    setTimeout(() => setShowCopied(false), 2000);
  };

  const handleDeleteEntry = async () => {
    try {
      await deleteAudio(entry.id);
    } catch (error) {
      console.error("Failed to delete entry:", error);
      notify({ title: t("settings.history.deleteError") });
    }
  };

  const formattedDate = formatDateTime(String(entry.timestamp), i18n.language);

  let progressText = t("settings.history.transcribing");
  if (job?.state === "queued") {
    progressText = t("settings.history.waiting");
  } else if (job?.state === "running" && job.percent > 0) {
    progressText = t("settings.history.transcribingPercent", {
      percent: job.percent,
    });
  }

  return (
    <div className="vt-history__item">
      <div className="vt-history__head">
        <div className="vt-history__meta">
          <p className="vt-history__date">{formattedDate}</p>
          {entry.source_name && (
            <span className="vt-history__source" title={entry.source_name}>
              <FileAudio
                width={13}
                height={13}
                className="vt-history__source-icon"
              />
              <span className="vt-history__source-name">
                {entry.source_name}
              </span>
            </span>
          )}
        </div>
        <div className="vt-history__actions">
          <IconButton
            onClick={handleCopyText}
            disabled={!hasTranscription || retrying}
            title={t("settings.history.copyToClipboard")}
          >
            {showCopied ? (
              <Check width={16} height={16} />
            ) : (
              <Copy width={16} height={16} />
            )}
          </IconButton>
          <IconButton
            onClick={onToggleSaved}
            active={entry.saved}
            title={
              entry.saved
                ? t("settings.history.unsave")
                : t("settings.history.save")
            }
          >
            <Star
              width={16}
              height={16}
              fill={entry.saved ? "currentColor" : "none"}
            />
          </IconButton>
          {retrying ? (
            <IconButton
              onClick={() => {
                setCancelling(true);
                void cancelTranscription(entry.id);
              }}
              disabled={cancelling}
              title={t("settings.history.cancelTranscription")}
            >
              <X width={16} height={16} />
            </IconButton>
          ) : (
            <IconButton
              onClick={() => retryTranscription(entry.id)}
              title={t("settings.history.retranscribe")}
            >
              <RotateCcw width={16} height={16} />
            </IconButton>
          )}
          <IconButton
            onClick={handleDeleteEntry}
            title={t("settings.history.delete")}
          >
            <Trash2 width={16} height={16} />
          </IconButton>
        </div>
      </div>

      <div className="vt-history__body">
        <p
          ref={textRef}
          className={`vt-history__text ${
            retrying
              ? "vt-history__text--pending"
              : hasTranscription
                ? ""
                : "vt-history__text--muted"
          }`}
          style={
            folded
              ? {
                  display: "-webkit-box",
                  WebkitLineClamp: FOLDED_LINES,
                  WebkitBoxOrient: "vertical",
                  overflow: "hidden",
                }
              : undefined
          }
        >
          {retrying
            ? progressText
            : hasTranscription
              ? entry.transcription_text
              : entry.source_name
                ? // Cancelled, interrupted by a quit, or failed.
                  t("settings.history.notTranscribed")
                : t("settings.history.transcriptionFailed")}
        </p>
        {hasTranscription && !retrying && (expanded || overflows) && (
          <button
            type="button"
            onClick={() => setExpanded(!expanded)}
            className="vt-history__more"
          >
            {expanded
              ? t("settings.history.showLess")
              : t("settings.history.showMore")}
          </button>
        )}
      </div>

      <AudioPlayer onLoadRequest={handleLoadAudio} />
    </div>
  );
};
