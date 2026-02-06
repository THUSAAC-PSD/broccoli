# Broccoli Plugin System

## Overview

The plugin system allows extending the frontend application by injecting React
components into predefined slots throughout the UI. Plugins are modular,
type-safe, and support dynamic loading/unloading.

## Quick Start

### 1. Create Plugin Structure

```
src/plugins/my-plugin/
├── index.ts
└── components/
    └── MyComponent.tsx
```

### 2. Define Component

```tsx
// src/plugins/my-plugin/components/MyComponent.tsx
import { SidebarMenuItem, SidebarMenuButton } from '@/components/ui/sidebar';
import { Icon } from 'lucide-react';

export function MyComponent() {
  return (
    <SidebarMenuItem>
      <SidebarMenuButton onClick={() => alert('Hello!')}>
        <Icon />
        <span>My Button</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}
```

### 3. Create Plugin Manifest

```typescript
// src/plugins/my-plugin/index.ts
import type { PluginManifest, ComponentBundle } from '@broccoli/sdk';
import { MyComponent } from './components/MyComponent';

export const manifest: PluginManifest = {
  name: 'my-plugin',
  version: '1.0.0',
  slots: [
    {
      name: 'sidebar.footer',
      position: 'append',
      component: 'my/Component',
    },
  ],
};

export const components: ComponentBundle = {
  'my/Component': MyComponent,
};
```

### 4. Load Plugin

```tsx
// src/App.tsx
import { PluginLoader } from '@/components/PluginLoader';
import * as MyPlugin from '@/plugins/my-plugin';

const plugins = [MyPlugin];

function App() {
  return (
    <PluginRegistryProvider>
      <PluginLoader plugins={plugins} />
      <AppLayout>{/* your app */}</AppLayout>
    </PluginRegistryProvider>
  );
}
```

## Core Concepts

### Plugin

A plugin exports two objects:

- `manifest`: Metadata and slot configurations
- `components`: React components keyed by string identifiers

### Slot

A location in the UI where plugins inject components.

```tsx
<Slot name="sidebar.footer" as="div" />
```

### Slot Config

Defines where and how to render a component.

```typescript
{
  name: 'sidebar.footer',      // Slot to target
  position: 'append',          // How to inject
  component: 'my/Component',   // Component key
  priority: 0,                 // Render order (optional)
  condition: (ctx) => true,    // Conditional render (optional)
  props: {},                   // Props to pass (optional)
}
```

## Slot Positions

| Position  | Behavior                                |
| --------- | --------------------------------------- |
| `prepend` | Insert before existing content          |
| `append`  | Insert after existing content (default) |
| `before`  | Insert immediately before slot children |
| `after`   | Insert immediately after slot children  |
| `replace` | Replace all slot content                |
| `wrap`    | Wrap slot content with component        |

### Position Examples

**prepend/append:**

```
slot: [existing content]
prepend: [plugin] [existing content]
append: [existing content] [plugin]
```

**replace:**

```
slot: [existing content]
replace: [plugin]
```

**wrap:**

```tsx
// Plugin component receives children prop
export function Wrapper({ children }) {
  return <div className="wrapper">{children}</div>;
}

// Result: <div className="wrapper">[existing content]</div>
```

## Available Slots

### Sidebar

- `sidebar.header` - Top of sidebar
- `sidebar.content.before` - Before main content
- `sidebar.platform.menu` - Platform menu items
- `sidebar.groups` - Custom menu groups
- `sidebar.account.menu` - Account menu items
- `sidebar.content.after` - After main content
- `sidebar.footer` - Bottom of sidebar

### Navbar

- `navbar.brand` - Logo area
- `navbar.menu` - Main menu items
- `navbar.actions` - Action buttons (right side)
- `navbar.mobile.menu` - Mobile menu

### App

- `app.root` - Root wrapper (for providers)
- `app.overlay` - Overlay layer (for modals)

## Advanced Features

### Priority Ordering

Control render order when multiple plugins target the same slot. Higher priority
renders first.

```typescript
{
  priority: 100,  // Renders first
}

{
  priority: 0,    // Default, renders after 100
}

{
  priority: -10,  // Renders last
}
```

### Conditional Rendering

Show/hide components based on runtime conditions.

```typescript
// Simple condition
{
  condition: () => import.meta.env.DEV,
}

// Context-based condition
{
  condition: (ctx) => ctx?.user?.isAdmin === true,
}
```

Use with context:

```tsx
<Slot name="admin.panel" context={{ user: currentUser }} />
```

### Props Passing

Pass data to plugin components.

**In slot config:**

```typescript
{
  props: {
    variant: 'primary',
    label: 'Click Me',
  },
}
```

**In component:**

```tsx
interface Props {
  variant: string;
  label: string;
}

export function MyButton({ variant, label }: Props) {
  return <Button variant={variant}>{label}</Button>;
}
```

### Lifecycle Hooks

Execute code when plugin loads/unloads.

```typescript
export const manifest: PluginManifest = {
  name: 'my-plugin',
  version: '1.0.0',
  onInit: async () => {
    console.log('Plugin initialized');
    // Setup code here
  },
  onDestroy: async () => {
    console.log('Plugin destroyed');
    // Cleanup code here
  },
  slots: [
    /* ... */
  ],
};
```

### Dynamic Plugin Management

Load plugins at runtime:

```tsx
import { useDynamicPluginLoader } from '@/components/PluginLoader';

function PluginManager() {
  const { loadPlugin, unloadPlugin } = useDynamicPluginLoader();

  const handleLoad = async () => {
    const plugin = await import('@/plugins/dynamic-plugin');
    await loadPlugin(plugin);
  };

  const handleUnload = async () => {
    await unloadPlugin('dynamic-plugin');
  };

  return (
    <>
      <button onClick={handleLoad}>Load</button>
      <button onClick={handleUnload}>Unload</button>
    </>
  );
}
```

### Enable/Disable Plugins

Toggle plugins without unloading:

```tsx
import { usePluginRegistry } from '@broccoli/sdk/react';

function PluginToggle({ pluginName }) {
  const { enablePlugin, disablePlugin, isPluginEnabled } = usePluginRegistry();
  const enabled = isPluginEnabled(pluginName);

  return (
    <button
      onClick={() =>
        enabled ? disablePlugin(pluginName) : enablePlugin(pluginName)
      }
    >
      {enabled ? 'Disable' : 'Enable'}
    </button>
  );
}
```

## Utility Helpers

### Plugin Storage

Plugin-scoped localStorage wrapper.

```typescript
import { pluginStorage } from '@/lib/plugin-utils';

// Save data
pluginStorage.set('my-plugin', 'settings', { theme: 'dark' });

// Load data
const settings = pluginStorage.get('my-plugin', 'settings');

// Remove data
pluginStorage.remove('my-plugin', 'settings');

// Clear all plugin data
pluginStorage.clear('my-plugin');
```

### Plugin Logger

Namespaced console logger.

```typescript
import { createPluginLogger } from '@/lib/plugin-utils';

const logger = createPluginLogger('my-plugin');

logger.log('Message'); // [my-plugin] Message
logger.warn('Warning'); // [my-plugin] Warning
logger.error('Error'); // [my-plugin] Error
logger.debug('Debug'); // Only in dev mode
```

### Helper Functions

```typescript
import { createPlugin, createSlot } from '@/lib/plugin-utils';

// Create plugin
export const { manifest, components } = createPlugin({
  name: 'my-plugin',
  version: '1.0.0',
  slots: [
    /* ... */
  ],
  components: {
    /* ... */
  },
});

// Create slot config
const slot = createSlot({
  name: 'sidebar.footer',
  component: 'my/Component',
  priority: 10,
});
```

## API Reference

### Types

```typescript
interface PluginManifest {
  name: string;
  version: string;
  slots?: SlotConfig[];
  components?: Record<string, any>;
  onInit?: () => void | Promise<void>;
  onDestroy?: () => void | Promise<void>;
  enabled?: boolean;
}

interface SlotConfig<TContext = unknown> {
  name: string;
  position: 'append' | 'prepend' | 'replace' | 'before' | 'after' | 'wrap';
  component: string;
  target?: string;
  priority?: number;
  condition?: (context?: TContext) => boolean;
  props?: Record<string, unknown>;
}

interface ComponentBundle {
  [key: string]: ComponentType<any>;
}
```

### Components

**PluginLoader**

Automatically loads plugins on mount.

```tsx
<PluginLoader
  plugins={[Plugin1, Plugin2]}
  onLoad={() => console.log('Loaded')}
  onError={(name, error) => console.error(name, error)}
/>
```

**Slot**

Renders plugin components at specified location.

```tsx
<Slot
  name="slot.name"
  as="div"
  className="custom-class"
  context={{ data: 'value' }}
  slotProps={{ sharedProp: true }}
>
  Default Content
</Slot>
```

### Hooks

**usePluginRegistry**

Access the plugin registry.

```tsx
const {
  plugins,
  components,
  enabledPlugins,
  registerPlugin,
  enablePlugin,
  disablePlugin,
  isPluginEnabled,
  getSlots,
} = usePluginRegistry();
```

**usePluginComponent**

Get a specific component.

```tsx
const MyComponent = usePluginComponent('my/Component');
if (MyComponent) {
  return <MyComponent />;
}
```

**usePluginEnabled**

Check if plugin is enabled.

```tsx
const isEnabled = usePluginEnabled('my-plugin');
```

**useEnabledPlugins**

Get all enabled plugins.

```tsx
const enabledPlugins = useEnabledPlugins();
```

**useDynamicPluginLoader**

Load/unload plugins dynamically.

```tsx
const { loadPlugin, unloadPlugin } = useDynamicPluginLoader();
```

## Examples

### Theme Plugin

Adds theme toggle to sidebar.

```typescript
// src/plugins/theme-plugin/index.ts
import type { PluginManifest, ComponentBundle } from '@broccoli/sdk';
import { ThemeToggle } from './components/ThemeToggle';

export const manifest: PluginManifest = {
  name: 'theme-plugin',
  version: '1.0.0',
  slots: [
    {
      name: 'sidebar.footer',
      position: 'prepend',
      component: 'theme/ThemeToggle',
      priority: 100,
    },
  ],
};

export const components: ComponentBundle = {
  'theme/ThemeToggle': ThemeToggle,
};
```

```tsx
// src/plugins/theme-plugin/components/ThemeToggle.tsx
import { Moon, Sun } from 'lucide-react';
import { SidebarMenuItem, SidebarMenuButton } from '@/components/ui/sidebar';
import { useTheme } from '@/hooks/use-theme';

export function ThemeToggle() {
  const { theme, toggleTheme } = useTheme();

  return (
    <SidebarMenuItem>
      <SidebarMenuButton onClick={toggleTheme}>
        {theme === 'light' ? <Moon /> : <Sun />}
        <span>{theme === 'light' ? 'Dark' : 'Light'} Mode</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}
```

### Notification Plugin

Adds notification bell to navbar.

```typescript
// src/plugins/notification-plugin/index.ts
import type { PluginManifest, ComponentBundle } from '@broccoli/sdk';
import { NotificationButton } from './components/NotificationButton';

export const manifest: PluginManifest = {
  name: 'notification-plugin',
  version: '1.0.0',
  slots: [
    {
      name: 'navbar.actions',
      position: 'append',
      component: 'notifications/Button',
    },
  ],
};

export const components: ComponentBundle = {
  'notifications/Button': NotificationButton,
};
```

```tsx
// src/plugins/notification-plugin/components/NotificationButton.tsx
import { Bell } from 'lucide-react';
import { Button } from '@/components/ui/button';

export function NotificationButton() {
  const count = 3;

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
```

### Analytics Plugin (Wrapper)

Wraps app for event tracking.

```typescript
// src/plugins/analytics-plugin/index.ts
import type { PluginManifest, ComponentBundle } from '@broccoli/sdk';
import { AnalyticsTracker } from './components/AnalyticsTracker';

export const manifest: PluginManifest = {
  name: 'analytics-plugin',
  version: '1.0.0',
  slots: [
    {
      name: 'app.root',
      position: 'wrap',
      component: 'analytics/Tracker',
    },
  ],
  onInit: () => console.log('[Analytics] Initialized'),
};

export const components: ComponentBundle = {
  'analytics/Tracker': AnalyticsTracker,
};
```

```tsx
// src/plugins/analytics-plugin/components/AnalyticsTracker.tsx
import { useEffect, type ReactNode } from 'react';

export function AnalyticsTracker({ children }: { children: ReactNode }) {
  useEffect(() => {
    console.log('[Analytics] Page view tracked');

    const handleClick = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      if (target.tagName === 'BUTTON') {
        console.log('[Analytics] Click:', target.textContent);
      }
    };

    document.addEventListener('click', handleClick);
    return () => document.removeEventListener('click', handleClick);
  }, []);

  return <>{children}</>;
}
```

## Common Patterns

### Sidebar Button

```tsx
import { SidebarMenuItem, SidebarMenuButton } from '@/components/ui/sidebar';
import { Icon } from 'lucide-react';

export function SidebarButton() {
  return (
    <SidebarMenuItem>
      <SidebarMenuButton onClick={() => console.log('Clicked')}>
        <Icon />
        <span>Label</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}
```

### Navbar Button

```tsx
import { Button } from '@/components/ui/button';
import { Icon } from 'lucide-react';

export function NavbarButton() {
  return (
    <Button variant="ghost" size="icon">
      <Icon className="h-5 w-5" />
    </Button>
  );
}
```

### Conditional Plugin

```typescript
{
  condition: () => import.meta.env.DEV,
}

{
  condition: (ctx) => ctx?.user?.role === 'admin',
}
```
