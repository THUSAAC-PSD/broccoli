/**
 * Shared Tailwind CSS preset for Broccoli plugins.
 *
 * Plugins that generate their own Tailwind CSS should use this preset
 * in their `tailwind.config.js` to stay consistent with the host app's
 * design system:
 *
 * ```js
 * // tailwind.config.js
 * import { broccoliPreset } from '@broccoli/web-sdk/tailwind';
 * export default {
 *   presets: [broccoliPreset],
 *   content: ['./src/**\/*.{js,ts,jsx,tsx}'],
 * };
 * ```
 *
 * Plugin CSS must be wrapped in `@layer plugin` to avoid overriding
 * host utility classes. In your `src/styles.css`:
 *
 * ```css
 * @layer plugin {
 *   @tailwind utilities;
 * }
 * ```
 *
 * The `@layer plugin` ensures that plugin utilities always have lower
 * cascade priority than the host's unlayered Tailwind utilities. Without
 * this, dynamically loaded plugin CSS can override responsive variants
 * like `md:block` due to source order.
 *
 * The preset defines semantic color tokens that reference CSS custom
 * properties set by the host app's theme. This means plugin CSS works
 * in both light and dark modes without any extra configuration.
 */

export const broccoliPreset = {
  darkMode: ['class'] as const,
  theme: {
    extend: {
      borderRadius: {
        lg: 'var(--radius)',
        md: 'calc(var(--radius) - 2px)',
        sm: 'calc(var(--radius) - 4px)',
      },
      colors: {
        background: 'hsl(var(--background))',
        foreground: 'hsl(var(--foreground))',
        card: {
          DEFAULT: 'hsl(var(--card))',
          foreground: 'hsl(var(--card-foreground))',
        },
        popover: {
          DEFAULT: 'hsl(var(--popover))',
          foreground: 'hsl(var(--popover-foreground))',
        },
        primary: {
          DEFAULT: 'hsl(var(--primary))',
          foreground: 'hsl(var(--primary-foreground))',
        },
        secondary: {
          DEFAULT: 'hsl(var(--secondary))',
          foreground: 'hsl(var(--secondary-foreground))',
        },
        muted: {
          DEFAULT: 'hsl(var(--muted))',
          foreground: 'hsl(var(--muted-foreground))',
        },
        accent: {
          DEFAULT: 'hsl(var(--accent))',
          foreground: 'hsl(var(--accent-foreground))',
        },
        destructive: {
          DEFAULT: 'hsl(var(--destructive))',
          foreground: 'hsl(var(--destructive-foreground))',
        },
        border: 'hsl(var(--border))',
        input: 'hsl(var(--input))',
        ring: 'hsl(var(--ring))',
        chart: {
          1: 'hsl(var(--chart-1))',
          2: 'hsl(var(--chart-2))',
          3: 'hsl(var(--chart-3))',
          4: 'hsl(var(--chart-4))',
          5: 'hsl(var(--chart-5))',
        },
      },
    },
  },
};
