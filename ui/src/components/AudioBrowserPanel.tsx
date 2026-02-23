import { useEffect, useMemo, useState } from "react";

import type { AudioAnalysis, AudioAssetEntry } from "../types";

interface AudioBrowserPanelProps {
  directory: string;
  assets: AudioAssetEntry[];
  selectedAssetPath: string | null;
  preview: AudioAnalysis | null;
  loading: boolean;
  onScan: (directory: string) => void;
  onSelectAsset: (assetPath: string) => void;
  onImportAsset: (assetPath: string) => void;
}

function prettyBytes(sizeBytes: number): string {
  if (sizeBytes < 1024) {
    return `${sizeBytes} B`;
  }
  if (sizeBytes < 1024 * 1024) {
    return `${(sizeBytes / 1024).toFixed(1)} KB`;
  }
  return `${(sizeBytes / (1024 * 1024)).toFixed(2)} MB`;
}

export function AudioBrowserPanel({
  directory,
  assets,
  selectedAssetPath,
  preview,
  loading,
  onScan,
  onSelectAsset,
  onImportAsset
}: AudioBrowserPanelProps) {
  const [scanDirectory, setScanDirectory] = useState(directory);

  useEffect(() => {
    setScanDirectory(directory);
  }, [directory]);

  const peakBars = useMemo(() => {
    return preview?.peaks.peaks.slice(0, 72) ?? [];
  }, [preview]);

  return (
    <aside className="panel panel--audio-browser">
      <h2>Audio Browser</h2>
      <label className="field">
        <span>Scan Directory</span>
        <input
          value={scanDirectory}
          onChange={(event) => setScanDirectory(event.target.value)}
          placeholder="tmp/audio"
        />
      </label>
      <button type="button" className="pill" disabled={loading} onClick={() => onScan(scanDirectory)}>
        Scan Audio
      </button>

      <div className="audio-browser__list">
        {assets.length === 0 ? <p>No audio assets found.</p> : null}
        {assets.map((asset) => (
          <button
            key={asset.path}
            type="button"
            className={`audio-browser__item ${selectedAssetPath === asset.path ? "audio-browser__item--selected" : ""}`}
            onClick={() => onSelectAsset(asset.path)}
          >
            <span className="audio-browser__item-path">{asset.path}</span>
            <span className="audio-browser__item-meta">
              {asset.extension.toUpperCase()} • {prettyBytes(asset.size_bytes)}
            </span>
          </button>
        ))}
      </div>

      <div className="clip-editor__actions">
        <button
          type="button"
          className="pill"
          disabled={loading || !selectedAssetPath}
          onClick={() => {
            if (selectedAssetPath) {
              onImportAsset(selectedAssetPath);
            }
          }}
        >
          Import To Playlist
        </button>
      </div>

      {preview ? (
        <div className="audio-browser__preview">
          <p>
            <span className="label">Duration</span>
            <strong>{preview.duration_seconds.toFixed(2)}s</strong>
          </p>
          <p>
            <span className="label">Format</span>
            <strong>
              {preview.sample_rate} Hz • {preview.channels} ch
            </strong>
          </p>
          <p>
            <span className="label">Frames</span>
            <strong>{preview.total_frames.toLocaleString()}</strong>
          </p>
          <div className="audio-browser__waveform">
            {peakBars.map((peak, index) => (
              <span
                key={`${index}-${peak}`}
                style={{ height: `${Math.max(4, Math.round(peak * 52))}px` }}
              />
            ))}
          </div>
        </div>
      ) : null}
    </aside>
  );
}
