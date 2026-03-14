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

  const handleSubmit = () => {
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
            value={content}
            onChange={(e) => setContent(e.target.value)}
          />
          <p className="text-xs text-muted-foreground mt-2">
            Your question will be visible only to judges until they decide to
            make it public.
          </p>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => setOpen(false)}>
            Cancel
          </Button>
          <Button onClick={handleSubmit} disabled={!content.trim()}>
            Send
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
