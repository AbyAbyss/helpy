// Section-specific controls that don't fit the generic set.

import { useEffect, useState } from "react";
import type { BuddyStyle } from "../../bindings/BuddyStyle";
import { api, asCommandError, pickFile } from "../../lib/ipc";
import { Buddy } from "../../buddy/Buddy";
import { Button } from "../controls/controls";
import { useCtx } from "../context";
import { Icon } from "../icons";

export function useCustomBuddySrc(): string | null {
  const { store } = useCtx();
  const name = store.get<string | null>("buddy.customImage");
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    if (!name) setSrc(null);
    else api.getCustomBuddy().then(setSrc);
  }, [name]);
  return src;
}

const STYLES: { value: Exclude<BuddyStyle, "custom">; name: string; note: string }[] = [
  { value: "pip", name: "Pip", note: "The original" },
  { value: "orbit", name: "Orbit", note: "Calm and minimal" },
  { value: "spark", name: "Spark", note: "Bright, easy to spot" },
  { value: "pebble", name: "Pebble", note: "Dark and quiet" },
];

export function BuddyStylePicker() {
  const { store } = useCtx();
  const current = store.get<BuddyStyle>("buddy.style");
  const customSrc = useCustomBuddySrc();
  const upload = useUploadBuddy();
  const error = store.errors["buddy.style"];
  return (
    <div className="style-picker-wrap">
      <div className="style-picker" role="radiogroup" aria-label="Buddy style">
        {STYLES.map((s) => (
          <button
            key={s.value}
            type="button"
            role="radio"
            aria-checked={current === s.value}
            className="style-tile"
            onClick={() => store.set("buddy.style", s.value)}
          >
            <span className="style-tile-art">
              <Buddy style={s.value} size={40} activity="idle" animate={false} />
            </span>
            <span className="style-tile-name">{s.name}</span>
            <span className="style-tile-note">{s.note}</span>
          </button>
        ))}
        <button
          type="button"
          role="radio"
          aria-checked={current === "custom"}
          className="style-tile"
          data-empty={customSrc ? "no" : "yes"}
          onClick={() => (customSrc ? store.set("buddy.style", "custom") : upload())}
        >
          <span className="style-tile-art">
            {customSrc ? <Buddy style="custom" customSrc={customSrc} size={40} activity="idle" animate={false} /> : <Icon name="upload" size={20} />}
          </span>
          <span className="style-tile-name">Custom</span>
          <span className="style-tile-note">{customSrc ? "Your image" : "Upload SVG or PNG"}</span>
        </button>
      </div>
      {error && <p className="row-error">{error}</p>}
    </div>
  );
}

function useUploadBuddy() {
  const { store, toast } = useCtx();
  return async () => {
    const path = await pickFile({ title: "Choose a buddy image", name: "Images", extensions: ["svg", "png"] });
    if (!path) return;
    try {
      store.replace(await api.setCustomBuddy(path));
      toast("Custom buddy saved");
    } catch (e) {
      const err = asCommandError(e);
      toast(err.kind === "message" ? err.message : "That image couldn't be used.", "error");
    }
  };
}

export function CustomBuddyUpload() {
  const { store } = useCtx();
  const upload = useUploadBuddy();
  const has = !!store.get<string | null>("buddy.customImage");
  return (
    <Button onClick={upload} icon={<Icon name="upload" />}>
      {has ? "Replace image…" : "Upload image…"}
    </Button>
  );
}

export function OverlayCheckButton() {
  const { toast } = useCtx();
  return (
    <Button
      icon={<Icon name="monitor" />}
      onClick={async () => {
        await api.showOverlayCheck();
        const displays = await api.listDisplays();
        toast(`Frame shown on ${displays.length} display${displays.length === 1 ? "" : "s"}`);
      }}
    >
      Show frame
    </Button>
  );
}

export function ResetAllButton() {
  const { store, toast } = useCtx();
  const [confirming, setConfirming] = useState(false);
  if (!confirming) {
    return (
      <Button variant="danger" onClick={() => setConfirming(true)}>
        Reset all…
      </Button>
    );
  }
  return (
    <div className="confirm-inline">
      <span>Reset every section?</span>
      <Button variant="ghost" onClick={() => setConfirming(false)}>
        Cancel
      </Button>
      <Button
        variant="danger"
        onClick={async () => {
          store.replace(await api.resetAll());
          setConfirming(false);
          toast("All settings reset to defaults");
        }}
      >
        Reset all
      </Button>
    </div>
  );
}
