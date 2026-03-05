import { Bell } from 'lucide-react';
import { useState } from 'react';
import { Link } from 'react-router';

import { Button } from '@/components/ui/button';
import { useAuth } from '@/contexts/auth-context';

export function NotificationButton() {
  const [count] = useState(3); // TODO: 实现通知计数的动态获取
  const { user } = useAuth();

  return (
    <Button
      variant="default"
      size="icon"
      className="relative fixed bottom-8 right-8 z-50 flex items-center justify-center w-10 h-10 bg-sidebar-primary text-sidebar-primary-foreground rounded-full shadow-lg hover:bg-sidebar-primary/90 hover:text-sidebar-primary-foreground hover:shadow-xl hover:-translate-y-1 transition-all duration-200"
    >
      <Link to={!user ? '/login' : '/'}>
        <Bell className="h-5 w-5" />
        {count > 0 && (
          <span className="absolute -right-1 -top-1 flex h-5 w-5 items-center justify-center rounded-full bg-red-500 text-xs text-white">
            {count}
          </span>
        )}
      </Link>
    </Button>
  );
}
