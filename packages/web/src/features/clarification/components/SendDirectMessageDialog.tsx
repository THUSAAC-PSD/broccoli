import { useApiClient } from '@broccoli/web-sdk/api';
import {
  Button,
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  Input,
  Textarea,
} from '@broccoli/web-sdk/ui';
import { useQuery } from '@tanstack/react-query';
import { Check, Mail, Search } from 'lucide-react';
import { useMemo, useState } from 'react';

interface SendDirectMessageDialogProps {
  contestId: number;
  onSubmit: (content: string, recipientId: number) => void;
}

export function SendDirectMessageDialog({
  contestId,
  onSubmit,
}: SendDirectMessageDialogProps) {
  const apiClient = useApiClient();
  const [open, setOpen] = useState(false);
  const [content, setContent] = useState('');
  const [search, setSearch] = useState('');
  const [selectedUser, setSelectedUser] = useState<{
    user_id: number;
    username: string;
    is_deleted: boolean;
  } | null>(null);

  const { data: participants = [] } = useQuery({
    queryKey: ['contest-participants', contestId],
    queryFn: async () => {
      const { data } = await apiClient.GET('/contests/{id}/participants', {
        params: { path: { id: contestId } },
      });
      return data ?? [];
    },
    enabled: open,
  });

  const filtered = useMemo(() => {
    if (!search.trim()) return participants;
    const term = search.toLowerCase();
    return participants.filter(
      (p) =>
        !p.is_deleted &&
        (p.username.toLowerCase().includes(term) ||
          String(p.user_id).includes(term)),
    );
  }, [participants, search]);

  const MAX_LENGTH = 10000;
  const trimmed = content.trim();
  const isValid =
    trimmed.length > 0 && trimmed.length <= MAX_LENGTH && !!selectedUser;

  const handleSubmit = () => {
    if (!isValid) return;
    onSubmit(content, selectedUser!.user_id);
    setContent('');
    setSearch('');
    setSelectedUser(null);
    setOpen(false);
  };

  const handleOpenChange = (v: boolean) => {
    setOpen(v);
    if (!v) {
      setSearch('');
      setSelectedUser(null);
      setContent('');
    }
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogTrigger asChild>
        <Button variant="outline">
          <Mail className="h-4 w-4 mr-2" />
          Message
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Send Direct Message</DialogTitle>
        </DialogHeader>
        <div className="py-4 space-y-4">
          {/* Recipient selector */}
          <div className="space-y-2">
            <label className="text-sm font-medium">Recipient</label>
            {selectedUser ? (
              <div className="flex items-center gap-2 p-2 border rounded-md bg-muted/50">
                <Check className="h-4 w-4 text-green-600" />
                <span className="font-medium">{selectedUser.username}</span>
                <span className="text-xs text-muted-foreground">
                  (ID: {selectedUser.user_id})
                </span>
                <Button
                  variant="ghost"
                  size="sm"
                  className="ml-auto h-6 text-xs"
                  onClick={() => setSelectedUser(null)}
                >
                  Change
                </Button>
              </div>
            ) : (
              <>
                <div className="relative">
                  <Search
                    className="pointer-events-none absolute top-2.5 h-4 w-4 text-muted-foreground"
                    style={{ insetInlineStart: '0.625rem' }}
                  />
                  <Input
                    placeholder="Search participants..."
                    value={search}
                    onChange={(e) => setSearch(e.target.value)}
                    style={{ paddingInlineStart: '2.25rem' }}
                  />
                </div>
                <div className="max-h-36 overflow-y-auto border rounded-md">
                  {filtered.length === 0 ? (
                    <div className="p-3 text-sm text-muted-foreground text-center">
                      No participants found
                    </div>
                  ) : (
                    filtered.map((p) => (
                      <button
                        key={p.user_id}
                        type="button"
                        className="w-full px-3 py-2 text-left text-sm hover:bg-muted flex items-center justify-between"
                        onClick={() => setSelectedUser(p)}
                      >
                        <span>{p.username}</span>
                        <span className="text-xs text-muted-foreground">
                          ID: {p.user_id}
                        </span>
                      </button>
                    ))
                  )}
                </div>
              </>
            )}
          </div>

          {/* Message content */}
          <div className="space-y-2">
            <label className="text-sm font-medium">Message</label>
            <Textarea
              placeholder="Write a message to this participant..."
              className="min-h-[120px]"
              maxLength={MAX_LENGTH}
              value={content}
              onChange={(e) => setContent(e.target.value)}
            />
          </div>
          <div className="flex justify-between">
            <p className="text-xs text-muted-foreground">
              This message will only be visible to the selected participant.
            </p>
            <span
              className={`text-xs ${trimmed.length > MAX_LENGTH ? 'text-destructive' : 'text-muted-foreground'}`}
            >
              {trimmed.length}/{MAX_LENGTH}
            </span>
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => handleOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={handleSubmit} disabled={!isValid}>
            Send
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
