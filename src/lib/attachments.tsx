// Pictures attached to a question or an agent task: pasted from the
// clipboard or picked from disk, sent as base64 JPEG. Where `pdfs` is on,
// PDFs come along too, as their text.
import { useRef, useState, type ClipboardEvent } from "react";
import { api } from "./ipc";
import "./attachments.css";

/** A PDF the user attached, read to text. */
export type Doc = { name: string; text: string };

const isPdf = (f: File) => f.type === "application/pdf" || f.name.toLowerCase().endsWith(".pdf");

/** Longest edge sent to a model; larger pictures are scaled down. */
const MAX_EDGE = 1568;

/** A picture as base64 JPEG (no data: prefix), scaled to fit MAX_EDGE. */
async function toJpeg(file: Blob): Promise<string> {
  const bitmap = await createImageBitmap(file);
  const scale = Math.min(1, MAX_EDGE / Math.max(bitmap.width, bitmap.height));
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(bitmap.width * scale));
  canvas.height = Math.max(1, Math.round(bitmap.height * scale));
  const ctx = canvas.getContext("2d")!;
  // JPEG has no transparency: put transparent pictures on white.
  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
  bitmap.close();
  return canvas.toDataURL("image/jpeg", 0.85).split(",", 2)[1];
}

export function useAttachments(max: number, pdfs = false) {
  const [images, setImages] = useState<string[]>([]);
  const [documents, setDocuments] = useState<Doc[]>([]);
  const [error, setError] = useState<string | null>(null);

  const addPdfs = async (files: File[]) => {
    const room = max - documents.length;
    if (files.length > room) setError(`Up to ${max} PDFs`);
    for (const f of files.slice(0, Math.max(0, room))) {
      try {
        const text = await api.pdfText(f);
        setDocuments((prev) => [...prev, { name: f.name, text }].slice(0, max));
      } catch (e) {
        setError(String(e));
      }
    }
  };

  const add = async (files: Iterable<File>) => {
    const all = [...files];
    const pictures = all.filter((f) => f.type.startsWith("image/"));
    const docs = pdfs ? all.filter(isPdf) : [];
    if (!pictures.length && !docs.length) return;
    setError(null);
    if (docs.length) await addPdfs(docs);
    if (!pictures.length) return;
    const room = max - images.length;
    if (pictures.length > room) setError(max === 1 ? "One picture at a time" : `Up to ${max} pictures`);
    try {
      const added = await Promise.all(pictures.slice(0, Math.max(0, room)).map(toJpeg));
      setImages((prev) => [...prev, ...added].slice(0, max));
    } catch {
      setError("Couldn't read that picture");
    }
  };

  /** For an input's onPaste: takes pictures (and PDFs), lets text paste as usual. */
  const onPaste = (e: ClipboardEvent) => {
    const files = [...e.clipboardData.files].filter((f) => f.type.startsWith("image/") || (pdfs && isPdf(f)));
    if (!files.length) return;
    e.preventDefault();
    void add(files);
  };

  return {
    images,
    documents,
    error,
    add,
    onPaste,
    remove: (i: number) => setImages((prev) => prev.filter((_, j) => j !== i)),
    removeDocument: (i: number) => setDocuments((prev) => prev.filter((_, j) => j !== i)),
    clear: () => {
      setImages([]);
      setDocuments([]);
      setError(null);
    },
  };
}

/** Thumbnails of attached pictures and names of PDFs; removable while still a draft. */
export function AttachmentStrip({ images, onRemove, documents = [], onRemoveDocument }: { images: string[]; onRemove?: (i: number) => void; documents?: string[]; onRemoveDocument?: (i: number) => void }) {
  if (!images.length && !documents.length) return null;
  return (
    <div className="attach-strip">
      {documents.map((name, i) => (
        <span key={`d${i}`} className="attach-doc" title={name}>
          <span className="attach-doc__name">{name}</span>
          {onRemoveDocument && (
            <button type="button" aria-label={`Remove ${name}`} onClick={() => onRemoveDocument(i)}>
              ×
            </button>
          )}
        </span>
      ))}
      {images.map((data, i) => (
        <span key={i} className="attach-thumb">
          <img src={`data:image/jpeg;base64,${data}`} alt={`Picture ${i + 1}`} />
          {onRemove && (
            <button type="button" aria-label={`Remove picture ${i + 1}`} onClick={() => onRemove(i)}>
              ×
            </button>
          )}
        </span>
      ))}
    </div>
  );
}

/** Opens the file picker for pictures (and PDFs with `pdfs`). `onOpen` runs
 * first (a window that closes on its own can stay open meanwhile). */
export function AttachButton({ onFiles, onOpen, disabled, className, pdfs }: { onFiles: (f: File[]) => void; onOpen?: () => void; disabled?: boolean; className?: string; pdfs?: boolean }) {
  const input = useRef<HTMLInputElement>(null);
  return (
    <>
      <button
        type="button"
        className={className ?? "attach-btn"}
        aria-label={pdfs ? "Attach a picture or PDF" : "Attach a picture"}
        title={disabled ? "Pictures can be added once it has finished" : pdfs ? "Attach a picture or PDF (or paste one)" : "Attach a picture (or paste one)"}
        disabled={disabled}
        onClick={() => {
          onOpen?.();
          input.current?.click();
        }}
      >
        <svg viewBox="0 0 20 20" width="16" height="16" aria-hidden="true">
          <path d="M14.5 9.5 9.8 14.2a3.2 3.2 0 0 1-4.5-4.5l5.3-5.3a2.1 2.1 0 0 1 3 3L8.3 12.7a1 1 0 0 1-1.5-1.5L11.5 6.5" />
        </svg>
      </button>
      <input
        ref={input}
        type="file"
        accept={pdfs ? "image/*,application/pdf,.pdf" : "image/*"}
        multiple
        hidden
        onChange={(e) => {
          onFiles([...(e.target.files ?? [])]);
          e.target.value = "";
        }}
      />
    </>
  );
}
