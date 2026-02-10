import { Ban, FileQuestion, AlertTriangle, Construction, Home } from 'lucide-react'
import React from 'react'; 
import { Button } from '@/components/ui/button';

interface ErrorPageProps {
  code: number | string; 
  message?: string;      
  onRetry?: () => void;  
  onBack?: () => void;   
}

const ErrorConfig = {
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
    '501': {
        icon: Construction,
        title: '501 Not Implemented',
        desc: 'This functionality has not been implemented yet.',
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
