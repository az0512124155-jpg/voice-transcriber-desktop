import React, { useCallback, useEffect, useState } from "react";
import { commands, type PhoneLinkStatus } from "../../bindings";

const DEFAULT_STATUS: PhoneLinkStatus = {
  running: false,
  port: 38381,
  code: null,
  fingerprint: null,
};

/** Opt-in HTTPS bridge for the Android lecture notebook. */
export const PhoneLinkCard: React.FC = () => {
  const [status, setStatus] = useState<PhoneLinkStatus>(DEFAULT_STATUS);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const refresh = useCallback(async () => {
    setStatus(await commands.phoneLinkStatus());
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const start = async () => {
    setBusy(true);
    setError("");
    const result = await commands.phoneLinkStart(null);
    if (result.status === "ok") setStatus(result.data);
    else setError(result.error);
    setBusy(false);
  };

  const stop = async () => {
    setBusy(true);
    setError("");
    const result = await commands.phoneLinkStop();
    if (result.status === "ok") setStatus(result.data);
    else setError(result.error);
    setBusy(false);
  };

  return (
    <div className="vt-p-4 vt-flex vt-flex-col vt-gap-3">
      <div>
        <p className="vt-text-sm vt-font-medium">
          חיבור למחברת השיעור בטלפון
        </p>
        <p className="vt-text-xs vt-text-secondary vt-mt-1">
          שרת HTTPS מקומי, כבה בכניסה למחשב. רק טקסט עובר — שום אודיו לא נשלח
          מהטלפון. דורש ספק AI מוגדר בהגדרות כדי לענות על שאלות ולכתוב סיכומים.
        </p>
      </div>

      {status.running && status.code && (
        <div className="vt-rounded-lg vt-border vt-border-logo-primary/40 vt-p-3">
          <p className="vt-text-xs vt-text-secondary">קוד זיווג חד־פעמי</p>
          <p className="vt-text-2xl vt-tracking-widest vt-font-medium vt-mt-1">
            {status.code}
          </p>
          <p className="vt-text-xs vt-text-secondary vt-mt-2">
            תוקף 5 דקות · נעילה אחרי 5 ניסיונות שגויים
          </p>
          <p className="vt-text-xs vt-mt-3">
            בטלפון: מחברת שיעור ← חבר למחשב. האפליקציה מאתרת את המחשב לבד —
            כל מה שצריך להקליד הוא הקוד הזה.
          </p>
          <p className="vt-text-xs vt-text-secondary vt-mt-1">
            אין צורך בהעתקת כתובת IP או טביעת אצבע. שני המכשירים חייבים להיות
            על אותה רשת Wi-Fi.
          </p>
          {status.fingerprint && (
            <details className="vt-mt-2">
              <summary className="vt-text-xs vt-text-secondary vt-cursor-pointer">
                פרטים טכניים
              </summary>
              <p className="vt-text-xs vt-mt-1 vt-break-all">
                טביעת אישור: <code>{status.fingerprint}</code>
              </p>
            </details>
          )}
        </div>
      )}

      {error && <p className="vt-text-xs vt-text-danger">{error}</p>}

      <div className="vt-flex vt-gap-2">
        <button
          type="button"
          disabled={busy || status.running}
          onClick={() => void start()}
          className="vt-button vt-button--primary"
        >
          הפעל חיבור
        </button>
        <button
          type="button"
          disabled={busy || !status.running}
          onClick={() => void stop()}
          className="vt-button"
        >
          כבה
        </button>
      </div>
    </div>
  );
};
