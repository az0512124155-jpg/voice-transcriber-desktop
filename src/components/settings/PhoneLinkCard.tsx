import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type PhoneLinkStatus } from "../../bindings";
import { Button } from "../ui/Button";

const DEFAULT_STATUS: PhoneLinkStatus = {
  running: false,
  port: 38381,
  code: null,
  fingerprint: null,
};

/**
 * Opt-in HTTPS bridge for the Android lecture notebook.
 *
 * Built from the kit's own classes (`.vt-row`, `Button`). An earlier version
 * used `vt-button` / `vt-p-4` / `vt-flex`, which are not part of this
 * stylesheet — Tailwind here has no `vt` prefix, so every one of those classes
 * was dead. The start button rendered as bare text with no fill or border,
 * which is why it could not be found.
 */
export const PhoneLinkCard: React.FC = () => {
  const { t } = useTranslation();
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
    <>
      <div className="vt-row">
        <div className="vt-row__text">
          <p className="vt-row__title">{t("settings.notebook.link.rowTitle")}</p>
          <p className="vt-row__description">
            {t("settings.notebook.link.rowDescription")}
          </p>
        </div>
        <div className="vt-row__control">
          {status.running ? (
            <Button
              variant="secondary"
              onClick={() => void stop()}
              disabled={busy}
            >
              {t("settings.notebook.link.stop")}
            </Button>
          ) : (
            <Button
              variant="primary"
              onClick={() => void start()}
              disabled={busy}
            >
              {t("settings.notebook.link.start")}
            </Button>
          )}
        </div>
      </div>

      {status.running && status.code && (
        <div className="vt-row vt-row--stacked">
          <p className="vt-row__description">
            {t("settings.notebook.link.codeLabel")}
          </p>
          <p className="vt-phone-code">{status.code}</p>
          <p className="vt-row__description">
            {t("settings.notebook.link.codeHint")}
          </p>
        </div>
      )}

      {status.running && status.fingerprint && (
        <div className="vt-row vt-row--stacked">
          <details>
            <summary className="vt-row__description vt-phone-details">
              {t("settings.notebook.link.technical")}
            </summary>
            <p className="vt-row__description vt-phone-fingerprint">
              {t("settings.notebook.link.fingerprint")}{" "}
              <code>{status.fingerprint}</code>
            </p>
          </details>
        </div>
      )}

      {error && (
        <div className="vt-row vt-row--stacked">
          <p className="vt-row__description vt-phone-error">{error}</p>
        </div>
      )}
    </>
  );
};
