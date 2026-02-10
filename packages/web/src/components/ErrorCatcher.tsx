import { 
  AlertTriangle, 
  Ban, 
  FileQuestion, 
  ServerCrash, 
  Home, 
  Construction, 
  Lock, 
  FileWarning, 
  Timer, 
  CloudOff, 
  Activity, 
  Coffee 
} from 'lucide-react';
import React from 'react'; 
import { Button } from '@/components/ui/button';

interface ErrorPageProps {
  code: number | string; 
  message?: string;      
  onRetry?: () => void;  
  onBack?: () => void;   
}

const ErrorConfig = {
    '400': {
        icon: FileWarning,
        title: '400 Bad Request',
        desc: 'The server could not understand the request due to invalid syntax.',
    },
    '401': {
        icon: Lock,
        title: '401 Unauthorized',
        desc: 'You need to be logged in to access this resource.',
    },
    '403': {
        icon: Ban,
        title: '403 Forbidden',
        desc: 'Sorry, you do not have permission to access this page.',
    },
    '404': {
        icon: FileQuestion,
        title: '404 Not Found',
        desc: 'The requested resource could not be found on the server.',
    },
    '408': {
        icon: Timer, 
        title: '408 Request Timeout',
        desc: 'The server timed out waiting for the request.',
    },
    '418': {
        icon: Coffee,
        title: '418 I\'m a teapot',
        desc: 'The server refuses the attempt to brew coffee with a teapot.',
    },
    '429': {
        icon: Activity,
        title: '429 Too Many Requests',
        desc: 'You have sent too many requests in a given amount of time. Please try again later.',
    },
    '500': {
        icon: ServerCrash,
        title: '500 Internal Server Error',
        desc: 'Something went wrong on our end. Please try again later.',
    },
    '501': {
        icon: Construction,
        title: '501 Not Implemented',
        desc: 'This functionality has not been implemented yet.',
    }, 
    '502': {
        icon: CloudOff,
        title: '502 Bad Gateway',
        desc: 'The server received an invalid response from the upstream server.',
    },
    '503': {
        icon: ServerCrash, 
        title: '503 Service Unavailable',
        desc: 'The server is currently unavailable (overloaded or down for maintenance).',
    },
    '504': {
        icon: Timer,
        title: '504 Gateway Timeout',
        desc: 'The server took too long to respond.',
    },
    default: {
        icon: AlertTriangle,
        title: 'Unknown Error',
        desc: 'Something went wrong.',
    }
}

export function ErrorCatcher({ code, message, onRetry, onBack }: ErrorPageProps) {
    const config = ErrorConfig[String(code)] || ErrorConfig['default'];
    const description = message || config.desc;
    const Icon = config.icon;

    return (
      <div className="flex flex-col items-center justify-center min-h-[60vh] px-4 text-center space-y-6">
        <div className="p-6 rounded-full bg-muted">
          <Icon className="w-12 h-12 text-muted-foreground" />
        </div>

        <div className="space-y-2">
          <h1 className="text-3xl font-bold tracking-tighter sm:text-4xl">
            {config.title}
          </h1>
          <p className="text-gray-500 md:text-xl/relaxed dark:text-gray-400 max-w-[600px]">
            {description}
          </p>
        </div>
        <div className="mt-8 flex justify-center gap-4">
          <Button onClick={() => window.location.href = '/'} variant="default">
            <Home className="mr-2 h-4 w-4" />
            Return to Home Page
          </Button>
        </div>
      </div>
    );
}
