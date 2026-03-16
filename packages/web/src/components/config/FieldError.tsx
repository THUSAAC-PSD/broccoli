export function FieldError({ message }: Readonly<{ message?: string }>) {
  if (!message) return null;
  return <p className="text-xs text-destructive mt-1">{message}</p>;
}
