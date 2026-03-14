# 🥦 Broccoli

A monorepo project with a plugin system using slot architecture, built with
React, TypeScript, and Vite.

## Project Structure

```
broccoli/
├── packages/
│   ├── sdk/              # Core SDK for plugin system
│   │   ├── src/
│   │   │   ├── index.ts         # Main SDK exports
│   │   │   ├── react.tsx        # React-specific hooks and components
│   │   │   ├── types/
│   │   │   │   └── index.ts     # TypeScript type definitions
│   │   │   └── components/      # Core components
│   │   ├── package.json
│   │   └── tsconfig.json
│   │
│   └── web/              # Frontend application
│       ├── src/
│       │   ├── components/
│       │   │   └── ui/          # ShadCN UI components
│       │   │       └── button.tsx
│       │   ├── plugins/         # Plugin implementations
│       │   │   └── amazing-button/
│       │   │       ├── index.ts
│       │   │       └── components/
│       │   │           └── AmazingButton.tsx
│       │   ├── lib/
│       │   │   └── utils.ts     # Utility functions
│       │   ├── App.tsx
│       │   └── main.tsx
│       ├── package.json
│       ├── vite.config.ts
│       ├── tailwind.config.js
│       └── tsconfig.json
│
├── tsconfig.base.json    # Shared TypeScript configuration
├── pnpm-workspace.yaml   # PNPM workspace configuration
└── package.json          # Root package.json
```

## Features

### SDK Package (`@broccoli/web-sdk`)

The SDK provides a plugin system with:

- **Plugin Registry**: Context-based plugin management
- **Slot System**: Component injection with multiple positions:
  - `after` - Add after existing content
  - `replace` - Replace existing content
  - `before` - Add before existing content
- **TypeScript Support**: Full type definitions
- **React Hooks**: `usePluginRegistry()`, `usePluginComponent()`

#### SDK API

```typescript
import {
  PluginRegistryProvider,
  type ComponentBundle,
  type PluginModule,
} from '@broccoli/web-sdk/plugin';
import { Slot } from '@broccoli/web-sdk/slot';

function MyButton() {
  return null;
}

const components: ComponentBundle = {
  'components/MyButton': MyButton,
};

const myPlugin: PluginModule = {
  manifest: {
    id: 'my-plugin',
    name: 'my-plugin',
    version: '1.0.0',
    components: {
      'components/MyButton': 'MyButton',
    },
    slots: [
      {
        name: 'slots.header',
        position: 'append',
        component: 'components/MyButton',
      },
    ],
  },
  MyButton,
};

function App() {
  return (
    <PluginRegistryProvider
      backendUrl="http://127.0.0.1:3000"
      pluginModules={[myPlugin]}
    >
      <Slot name="slots.header" />
    </PluginRegistryProvider>
  );
}
```

### Web Package (`@broccoli/web`)

The web application demonstrates:

- **Plugin Usage**: Example plugin (`amazing-button`)
- **ShadCN UI Integration**: Pre-configured with Tailwind CSS
- **Slot Implementation**: Header slot with plugin injection
- **TypeScript**: Full type safety with path aliases

## Getting Started

### Prerequisites

- Node.js 18+ (recommended: 20+)
- pnpm 8+

### Installation

```bash
# Install dependencies
pnpm install

# Build SDK
pnpm --filter @broccoli/web-sdk build

# Or build all packages
pnpm build
```

### Development

```bash
# Start all packages in development mode
pnpm dev

# Start only web
pnpm --filter @broccoli/web dev

# Start only SDK in watch mode
pnpm --filter @broccoli/web-sdk dev
```

### Building

```bash
# Build all packages
pnpm build

# Build specific package
pnpm --filter @broccoli/web-sdk build
pnpm --filter @broccoli/web build
```

## Creating a Plugin

1. **Create plugin directory** in `packages/web/src/plugins/`:

```
plugins/
└── my-plugin/
    ├── index.ts
    └── components/
        └── MyComponent.tsx
```

2. **Define a plugin module** (`index.ts`):

```typescript
import type { ComponentBundle, PluginModule } from '@broccoli/web-sdk/plugin';
import { MyComponent } from './components/MyComponent';

export const components: ComponentBundle = {
  'components/MyComponent': MyComponent,
};

export const plugin: PluginModule = {
  manifest: {
    id: 'my-plugin',
    name: 'my-plugin',
    version: '1.0.0',
    components: {
      'components/MyComponent': 'MyComponent',
    },
    slots: [
      {
        name: 'slots.header',
        position: 'after',
        component: 'components/MyComponent',
      },
    ],
  },
  MyComponent,
};
```

3. **Provide the plugin module** in `App.tsx`:

```typescript
import { PluginRegistryProvider } from '@broccoli/web-sdk/plugin';
import { plugin as myPlugin } from './plugins/my-plugin';

function App() {
  return (
    <PluginRegistryProvider
      backendUrl="http://127.0.0.1:3000"
      pluginModules={[myPlugin]}
    >
      <AppContent />
    </PluginRegistryProvider>
  );
}
```

## Technology Stack

- **Build Tool**: Vite (Rolldown)
- **Framework**: React 19
- **Language**: TypeScript 5.9
- **Styling**: Tailwind CSS 3.4
- **UI Components**: ShadCN UI
- **Package Manager**: pnpm
- **Monorepo**: pnpm workspaces

## Scripts

```bash
pnpm dev      # Start development servers for all packages
pnpm build    # Build all packages
pnpm lint     # Lint all packages
pnpm test     # Run tests for all packages
```

## License

MIT
