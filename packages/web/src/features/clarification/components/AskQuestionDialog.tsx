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
import { MessageCircle } from 'lucide-react';
import { useState } from 'react';

interface AskQuestionDialogProps {
  onSubmit: (content: string) => void;
}

export function AskQuestionDialog({ onSubmit }: AskQuestionDialogProps) {
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
        <Button>
          <MessageCircle className="h-4 w-4 mr-2" />
          Ask Question
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Ask a Question</DialogTitle>
        </DialogHeader>
        <div className="py-4">
          <Textarea
            placeholder="Describe your issue clearly (e.g., Problem ID, specific error)..."
            className="min-h-[150px]"
            maxLength={MAX_LENGTH}
            value={content}
            onChange={(e) => setContent(e.target.value)}
          />
          <div className="flex justify-between mt-2">
            <p className="text-xs text-muted-foreground">
              Your question will be visible only to judges until they decide to
              make it public.
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
            Send
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
