import {
  Button,
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  Textarea,
} from '@broccoli/web-sdk/ui';
import { Megaphone } from 'lucide-react';
import { useState } from 'react';

interface PostAnnouncementDialogProps {
  onSubmit: (content: string) => void;
}

export function PostAnnouncementDialog({
  onSubmit,
}: PostAnnouncementDialogProps) {
  const [open, setOpen] = useState(false);
  const [content, setContent] = useState('');

  const MAX_LENGTH = 10000;
  const trimmed = content.trim();
  const isValid = trimmed.length > 0 && trimmed.length <= MAX_LENGTH;

  const handleSubmit = () => {
    if (!isValid) return;
    onSubmit(content);
    setContent('');
    setOpen(false);
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button variant="outline">
          <Megaphone className="h-4 w-4 mr-2" />
          Announcement
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Post Announcement</DialogTitle>
        </DialogHeader>
        <div className="py-4">
          <Textarea
            placeholder="Write an announcement visible to all participants..."
            className="min-h-[150px]"
            maxLength={MAX_LENGTH}
            value={content}
            onChange={(e) => setContent(e.target.value)}
          />
          <div className="flex justify-between mt-2">
            <p className="text-xs text-muted-foreground">
              This announcement will be immediately visible to all participants.
            </p>
            <span
              className={`text-xs ${trimmed.length > MAX_LENGTH ? 'text-destructive' : 'text-muted-foreground'}`}
            >
              {trimmed.length}/{MAX_LENGTH}
            </span>
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => setOpen(false)}>
            Cancel
          </Button>
          <Button onClick={handleSubmit} disabled={!isValid}>
            Post
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
