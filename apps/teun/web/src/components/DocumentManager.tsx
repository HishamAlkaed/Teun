import { useCallback, useEffect, useRef, useState } from "react";
import {
  type AdminDocument,
  listAdminDocuments,
  uploadDocuments,
  deleteDocument,
} from "../lib/api";

const POLL_INTERVAL_MS = 3000;
const ACTIVE_STATUSES = new Set(["pending", "indexing"]);

export function DocumentManager() {
  const [documents, setDocuments] = useState<AdminDocument[]>([]);
  const [loading, setLoading] = useState(true);
  const [uploading, setUploading] = useState(false);
  const [dragActive, setDragActive] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const pollTimerRef = useRef<number | undefined>(undefined);

  const refresh = useCallback(async () => {
    const list = await listAdminDocuments();
    setDocuments(list);
    setLoading(false);

    if (pollTimerRef.current !== undefined) {
      window.clearTimeout(pollTimerRef.current);
      pollTimerRef.current = undefined;
    }

    const hasActive = list.some((d) => ACTIVE_STATUSES.has(d.status));
    if (hasActive) {
      pollTimerRef.current = window.setTimeout(() => void refresh(), POLL_INTERVAL_MS);
    }
  }, []);

  useEffect(() => {
    void refresh();
    return () => {
      if (pollTimerRef.current !== undefined) {
        window.clearTimeout(pollTimerRef.current);
      }
    };
  }, [refresh]);

  const handleFiles = async (files: FileList | null) => {
    if (!files || files.length === 0) return;
    setUploading(true);
    setError(null);
    try {
      await uploadDocuments(files);
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Upload mislukt");
    } finally {
      setUploading(false);
      if (fileInputRef.current) fileInputRef.current.value = "";
    }
  };

  const handleDrop = (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    setDragActive(false);
    void handleFiles(e.dataTransfer.files);
  };

  const handleDelete = async (doc: AdminDocument) => {
    if (!window.confirm(`Weet je zeker dat je "${doc.filename}" wilt verwijderen?`)) {
      return;
    }
    setError(null);
    try {
      await deleteDocument(doc.id);
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Verwijderen mislukt");
    }
  };

  return (
    <div>
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-sm font-medium text-text-secondary">
          Documenten ({documents.length})
        </h2>
        <button
          onClick={() => fileInputRef.current?.click()}
          disabled={uploading}
          className="px-4 py-2 text-xs font-medium text-white bg-accent rounded-lg hover:bg-accent/90 disabled:opacity-50 cursor-pointer"
        >
          {uploading ? "Uploaden..." : "Documenten uploaden"}
        </button>
        <input
          ref={fileInputRef}
          type="file"
          accept="application/pdf"
          multiple
          className="hidden"
          onChange={(e) => void handleFiles(e.target.files)}
        />
      </div>

      {error && (
        <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-xs text-red-700">
          {error}
        </div>
      )}

      <div
        onDragOver={(e) => {
          e.preventDefault();
          setDragActive(true);
        }}
        onDragLeave={() => setDragActive(false)}
        onDrop={handleDrop}
        onClick={() => fileInputRef.current?.click()}
        className={`mb-6 border-2 border-dashed rounded-lg p-6 text-center text-xs cursor-pointer transition-colors ${
          dragActive
            ? "border-accent bg-accent/5 text-accent"
            : "border-border text-text-tertiary hover:border-border-light"
        }`}
      >
        {uploading
          ? "Bezig met uploaden..."
          : "Sleep PDF-bestanden hierheen of klik om te bladeren"}
      </div>

      {loading ? (
        <div className="text-xs text-text-tertiary">Laden...</div>
      ) : documents.length === 0 ? (
        <div className="text-center py-12 text-xs text-text-tertiary">
          Nog geen documenten geüpload
        </div>
      ) : (
        <div className="bg-surface border border-border rounded-lg overflow-x-auto">
          <table className="w-full">
            <thead>
              <tr className="border-b border-border-light">
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary">
                  Bestand
                </th>
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary">
                  Status
                </th>
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary">
                  Grootte
                </th>
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary">
                  Pagina&rsquo;s
                </th>
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary">
                  Fragmenten
                </th>
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary">
                  Geïndexeerd
                </th>
                <th className="text-right px-4 py-3 text-xs font-medium text-text-tertiary">
                  Acties
                </th>
              </tr>
            </thead>
            <tbody>
              {documents.map((doc) => (
                <tr key={doc.id} className="border-b border-border-light last:border-0">
                  <td className="px-4 py-3 text-xs text-text-primary align-top break-all">
                    <a
                      href={`/api/teun/documents/${encodeURIComponent(doc.filename)}/pdf`}
                      target="_blank"
                      rel="noreferrer"
                      className="text-accent hover:underline"
                    >
                      {doc.filename}
                    </a>
                  </td>
                  <td className="px-4 py-3 align-top">
                    <StatusBadge status={doc.status} errorMessage={doc.error_message} />
                  </td>
                  <td className="px-4 py-3 text-xs text-text-secondary align-top">
                    {formatSize(doc.size_bytes)}
                  </td>
                  <td className="px-4 py-3 text-xs text-text-secondary align-top">
                    {doc.page_count || "-"}
                  </td>
                  <td className="px-4 py-3 text-xs text-text-secondary align-top">
                    {doc.chunk_count || "-"}
                  </td>
                  <td className="px-4 py-3 text-xs text-text-secondary align-top">
                    {doc.indexed_at ? formatDate(doc.indexed_at) : "-"}
                  </td>
                  <td className="px-4 py-3 text-xs text-right align-top">
                    <button
                      onClick={() => void handleDelete(doc)}
                      className="text-text-tertiary hover:text-red-600 cursor-pointer"
                    >
                      Verwijder
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

function StatusBadge({
  status,
  errorMessage,
}: {
  status: string;
  errorMessage: string | null;
}) {
  const styles: Record<string, string> = {
    pending: "bg-gray-100 text-gray-600",
    indexing: "bg-blue-100 text-blue-700",
    indexed: "bg-emerald-100 text-emerald-700",
    error: "bg-red-100 text-red-700",
  };
  const labels: Record<string, string> = {
    pending: "Wachtend",
    indexing: "Bezig",
    indexed: "Geïndexeerd",
    error: "Fout",
  };

  return (
    <div>
      <span
        title={status === "error" ? errorMessage || undefined : undefined}
        className={`inline-block px-2 py-0.5 rounded text-xs font-medium ${styles[status] || styles.pending}`}
      >
        {labels[status] || status}
      </span>
      {status === "error" && errorMessage && (
        <div className="mt-1 text-xs text-red-600 max-w-xs">{errorMessage}</div>
      )}
    </div>
  );
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDate(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleDateString("nl-NL", {
    day: "numeric",
    month: "short",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
