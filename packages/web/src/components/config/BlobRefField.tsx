import { useApiFetch } from '@broccoli/web-sdk/api';
import { Button, Label } from '@broccoli/web-sdk/ui';
import { File, Loader2, Upload, X } from 'lucide-react';
import { useCallback, useRef, useState } from 'react';

interface BlobRefValue {
  filename: string;
  hash: string;
}

export function BlobRefField({
  label,
  description,
  value,
  onChange,
  isExplicit,
}: Readonly<{
  label: string;
  description?: string;
  value: BlobRefValue | undefined;
  onChange: (v: BlobRefValue | undefined) => void;
  isExplicit: boolean;
}>) {
  const apiFetch = useApiFetch();
  const inputRef = useRef<HTMLInputElement>(null);
  const [uploading, setUploading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [uploadingName, setUploadingName] = useState<string>('');

  const handleUpload = useCallback(
    async (file: globalThis.File) => {
      setError(null);
      setUploading(true);
      setUploadingName(file.name);

      try {
        const formData = new FormData();
        formData.append('file', file);

        const res = await apiFetch('/api/v1/config/upload', {
          method: 'POST',
          body: formData,
        });

        if (!res.ok) {
          const body = await res.json().catch(() => null);
          throw new Error(body?.message ?? `Upload failed (${res.status})`);
        }

        const data: { filename: string; content_hash: string } =
          await res.json();
        onChange({ filename: data.filename, hash: data.content_hash });
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Upload failed');
      } finally {
        setUploading(false);
        setUploadingName('');
        // Reset the input so re-selecting the same file triggers onChange
        if (inputRef.current) inputRef.current.value = '';
      }
    },
    [apiFetch, onChange],
  );

  const onFileChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (file) handleUpload(file);
    },
    [handleUpload],
  );

  return (
    <div className="flex flex-col gap-1.5">
      <div className="space-y-1">
        <div className="flex items-center gap-2">
          <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
            {label}
          </Label>
          {!isExplicit && (
            <span className="inline-flex items-center rounded-full border border-dashed px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
              Default
            </span>
          )}
        </div>
        {description && (
          <p className="text-xs text-muted-foreground">{description}</p>
        )}
      </div>

      <input
        ref={inputRef}
        type="file"
        className="hidden"
        onChange={onFileChange}
      />

      {uploading ? (
        /* Uploading state */
        <div className="flex items-center gap-2 rounded-md border border-dashed px-3 py-2 text-sm text-muted-foreground">
          <Loader2 className="h-3.5 w-3.5 animate-spin shrink-0" />
          <span className="truncate">{uploadingName}</span>
        </div>
      ) : value ? (
        /* Populated state */
        <div className="flex items-center gap-2">
          <div className="flex items-center gap-2 min-w-0 flex-1 rounded-md border bg-muted/40 px-3 py-2 text-sm">
            <File className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
            <span className="font-medium truncate">{value.filename}</span>
            <span className="text-muted-foreground font-mono text-xs truncate">
              {value.hash.slice(0, 12)}...
            </span>
          </div>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="shrink-0"
            onClick={() => inputRef.current?.click()}
          >
            <Upload className="h-3 w-3 mr-1.5" />
            Replace
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="shrink-0 h-8 w-8 text-muted-foreground hover:text-destructive"
            onClick={() => {
              setError(null);
              onChange(undefined);
            }}
          >
            <X className="h-3.5 w-3.5" />
          </Button>
        </div>
      ) : (
        /* Empty state */
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => inputRef.current?.click()}
        >
          <Upload className="h-3 w-3 mr-1.5" />
          Choose file
        </Button>
      )}

      {error && (
        <div className="flex items-center gap-2 text-sm text-destructive">
          <span>{error}</span>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-auto py-0.5 px-1.5 text-xs"
            onClick={() => inputRef.current?.click()}
          >
            Retry
          </Button>
        </div>
      )}
    </div>
  );
}
