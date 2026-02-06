import { Bell } from 'lucide-react';
import { useState } from 'react';

import { Button } from '@/components/ui/button';

export function NotificationButton() {
  const [count] = useState(3); // TODO: 实现通知计数的动态获取

  return (
    <Button variant="ghost" size="icon" className="relative">
      <Bell className="h-5 w-5" />
      {count > 0 && (
        <span className="absolute -right-1 -top-1 flex h-5 w-5 items-center justify-center rounded-full bg-red-500 text-xs text-white">
          {count}
        </span>
      )}
    </Button>
  );
}
