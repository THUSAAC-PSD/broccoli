import { Button } from '@/components/ui/button';

export function AmazingButton() {
  return (
    <Button variant="default" onClick={() => alert('Amazing!')}>
      Amazing Button
    </Button>
  );
}
