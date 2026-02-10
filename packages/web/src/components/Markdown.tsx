import type { ComponentPropsWithoutRef } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';
import 'katex/dist/katex.min.css';

interface MarkdownProps {
  children: string;
  className?: string;
}

export function Markdown({ children, className }: MarkdownProps) {
  return (
    <div className={className}>
      <ReactMarkdown
        children={children}
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[rehypeKatex]}
        components={{
          h1: ({ children }: ComponentPropsWithoutRef<'h1'>) => (
            <h1 className="text-2xl font-bold mt-6 mb-2">{children}</h1>
          ),
          h2: ({ children }: ComponentPropsWithoutRef<'h2'>) => (
            <h2 className="text-xl font-semibold mt-5 mb-2">{children}</h2>
          ),
          h3: ({ children }: ComponentPropsWithoutRef<'h3'>) => (
            <h3 className="text-lg font-semibold mt-4 mb-1.5">{children}</h3>
          ),
          p: ({ children }: ComponentPropsWithoutRef<'p'>) => (
            <p className="text-sm leading-relaxed mb-3 text-foreground/90">{children}</p>
          ),
          ul: ({ children }: ComponentPropsWithoutRef<'ul'>) => (
            <ul className="text-sm list-disc pl-6 mb-3 space-y-1 text-foreground/90">{children}</ul>
          ),
          ol: ({ children }: ComponentPropsWithoutRef<'ol'>) => (
            <ol className="text-sm list-decimal pl-6 mb-3 space-y-1 text-foreground/90">{children}</ol>
          ),
          li: ({ children }: ComponentPropsWithoutRef<'li'>) => (
            <li className="leading-relaxed">{children}</li>
          ),
          blockquote: ({ children }: ComponentPropsWithoutRef<'blockquote'>) => (
            <blockquote className="border-l-[3px] border-foreground/20 pl-4 my-3 text-sm text-foreground/70 italic">
              {children}
            </blockquote>
          ),
          hr: () => <hr className="my-6 border-border" />,
          a: ({ href, children }: ComponentPropsWithoutRef<'a'>) => (
            <a
              href={href}
              className="text-primary underline underline-offset-2 decoration-primary/40 hover:decoration-primary/80 transition-colors"
              target="_blank"
              rel="noopener noreferrer"
            >
              {children}
            </a>
          ),
          strong: ({ children }: ComponentPropsWithoutRef<'strong'>) => (
            <strong className="font-semibold text-foreground">{children}</strong>
          ),
          em: ({ children }: ComponentPropsWithoutRef<'em'>) => (
            <em className="italic">{children}</em>
          ),
          pre: ({ children }: ComponentPropsWithoutRef<'pre'>) => (
            <pre className="bg-muted/60 rounded-md p-4 my-3 overflow-x-auto text-sm leading-relaxed">
              {children}
            </pre>
          ),
          code({ children, className: codeClassName }) {
            const isBlock = !!codeClassName;
            if (isBlock) {
              return <code className="font-mono text-[13px]">{children}</code>;
            }
            return (
              <code className="bg-muted/80 text-destructive/80 rounded px-1.5 py-0.5 text-[13px] font-mono">
                {children}
              </code>
            );
          },
          table: ({ children }: ComponentPropsWithoutRef<'table'>) => (
            <div className="overflow-x-auto my-3 rounded-md border border-border">
              <table className="w-full text-sm">{children}</table>
            </div>
          ),
          thead: ({ children }: ComponentPropsWithoutRef<'thead'>) => (
            <thead className="bg-muted/40">{children}</thead>
          ),
          th: ({ children }: ComponentPropsWithoutRef<'th'>) => (
            <th className="px-3 py-2 text-left font-medium text-foreground/80 border-b border-border">
              {children}
            </th>
          ),
          td: ({ children }: ComponentPropsWithoutRef<'td'>) => (
            <td className="px-3 py-2 border-b border-border/50">{children}</td>
          ),
          img: ({ src, alt }: ComponentPropsWithoutRef<'img'>) => (
            <img src={src} alt={alt} className="rounded-md max-w-full my-3" />
          ),
          input: ({ checked, ...props }: ComponentPropsWithoutRef<'input'>) => (
            <input
              {...props}
              checked={checked}
              disabled
              className="mr-2 rounded accent-primary"
            />
          ),
        }}
      />
    </div>
  );
}
