import React, { useState, useEffect } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { arch } from "@tauri-apps/plugin-os";

import ModelSelector from "../model-selector";
import NoticeChip from "./NoticeChip";
import TranscriptionStatus from "./TranscriptionStatus";

interface FooterProps {
  onShowHistory: () => void;
}

/**
 * The status bar along the bottom of the window.
 *
 * It also carries the build's architecture next to the version. Two builds of
 * this app look identical on screen, so when someone reports that an update
 * "did nothing", the fastest way to tell a stale install from a real problem is
 * to read what this corner says: if it does not show the version just released,
 * the running binary is not the one that was installed.
 */
const archLabel = (value: string | null): string | null => {
  if (!value) return null;
  if (value === "x86_64") return "Intel";
  if (value === "aarch64") return "Apple silicon";
  if (value === "arm64") return "Apple silicon";
  return value;
};

const Footer: React.FC<FooterProps> = ({ onShowHistory }) => {
  const [version, setVersion] = useState("");
  const [build, setBuild] = useState<string | null>(null);

  useEffect(() => {
    const fetchBuild = async () => {
      try {
        setVersion(await getVersion());
      } catch (error) {
        console.error("Failed to get app version:", error);
      }
      try {
        setBuild(archLabel(await arch()));
      } catch {
        // Not running inside Tauri (a plain browser render): no arch to show.
        setBuild(null);
      }
    };

    fetchBuild();
  }, []);

  return (
    <div className="vt-statusbar">
      <div className="vt-statusbar__group">
        <ModelSelector />
        <TranscriptionStatus onShowHistory={onShowHistory} />
      </div>

      <div className="vt-statusbar__group">
        <NoticeChip />
        <span
          className="vt-statusbar__version"
          title={build ? `v${version} · ${build}` : undefined}
        >
          {version ? `v${version}${build ? ` · ${build}` : ""}` : (build ?? "")}
        </span>
      </div>
    </div>
  );
};

export default Footer;
