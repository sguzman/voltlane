import type { CSSProperties } from "react";

import type { Track } from "../types";

interface TrackLaneProps {
  track: Track;
  index: number;
  selected: boolean;
  selectedClipId: string | null;
  onSelect: (trackId: string) => void;
  onSelectClip: (trackId: string, clipId: string) => void;
  onToggleMute: (trackId: string, mute: boolean) => void;
  onToggleHidden: (trackId: string, hidden: boolean) => void;
  onToggleEnabled: (trackId: string, enabled: boolean) => void;
  onAddClip: (trackId: string, kind: Track["kind"]) => void;
  onAddEffect: (trackId: string) => void;
  onShiftTrack: (index: number, direction: "up" | "down") => void;
}

function clipTypeLabel(track: Track): string {
  if (track.kind === "chip") {
    return "Pattern";
  }
  return "Clip";
}

export function TrackLane({
  track,
  index,
  selected,
  selectedClipId,
  onSelect,
  onSelectClip,
  onToggleMute,
  onToggleHidden,
  onToggleEnabled,
  onAddClip,
  onAddEffect,
  onShiftTrack
}: TrackLaneProps) {
  return (
    <article
      className={`track-lane ${selected ? "track-lane--selected" : ""} ${track.hidden ? "track-lane--hidden" : ""}`}
      onClick={() => onSelect(track.id)}
      style={{ "--track-color": track.color } as CSSProperties}
    >
      <div className="track-lane__header">
        <div className="track-lane__identity">
          <span className="track-lane__index">{index + 1}</span>
          <div>
            <h3 className="track-lane__title">{track.name}</h3>
            <p className="track-lane__kind">{track.kind}</p>
          </div>
        </div>
        <div className="track-lane__toggles">
          <button
            type="button"
            className={`token ${track.enabled ? "" : "token--alert"}`}
            onClick={(event) => {
              event.stopPropagation();
              onToggleEnabled(track.id, !track.enabled);
            }}
          >
            {track.enabled ? "On" : "Off"}
          </button>
          <button
            type="button"
            className={`token ${track.mute ? "token--alert" : ""}`}
            onClick={(event) => {
              event.stopPropagation();
              onToggleMute(track.id, !track.mute);
            }}
          >
            M
          </button>
          <button
            type="button"
            className={`token ${track.hidden ? "token--alert" : ""}`}
            onClick={(event) => {
              event.stopPropagation();
              onToggleHidden(track.id, !track.hidden);
            }}
          >
            H
          </button>
        </div>
      </div>

      <div className="track-lane__clips">
        {track.clips.length === 0 ? <div className="clip clip--ghost">No clips yet</div> : null}
        {track.clips.map((clip) => {
          const width = Math.max(12, Math.round((clip.length_ticks / 1_920) * 100));
          return (
            <div
              key={clip.id}
              className={`clip ${clip.disabled ? "clip--disabled" : ""} ${selectedClipId === clip.id ? "clip--selected" : ""}`}
              style={{ width: `${width}%` }}
              onClick={(event) => {
                event.stopPropagation();
                onSelectClip(track.id, clip.id);
              }}
            >
              <span>{clip.name}</span>
            </div>
          );
        })}
      </div>

      <div className="track-lane__footer">
        <button
          type="button"
          className="mini"
          onClick={(event) => {
            event.stopPropagation();
            onAddClip(track.id, track.kind);
          }}
        >
          + {clipTypeLabel(track)}
        </button>
        <button
          type="button"
          className="mini"
          onClick={(event) => {
            event.stopPropagation();
            onAddEffect(track.id);
          }}
        >
          + FX
        </button>
        <button
          type="button"
          className="mini"
          onClick={(event) => {
            event.stopPropagation();
            onShiftTrack(index, "up");
          }}
        >
          Move Up
        </button>
        <button
          type="button"
          className="mini"
          onClick={(event) => {
            event.stopPropagation();
            onShiftTrack(index, "down");
          }}
        >
          Move Down
        </button>
      </div>
    </article>
  );
}
