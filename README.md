# 🥦 Broccoli

A monorepo project with a plugin system using slot architecture, built with React, TypeScript, and Vite.

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

### SDK Package (`@broccoli/sdk`)

The SDK provides a plugin system with:

-   **Plugin Registry**: Context-based plugin management
-   **Slot System**: Component injection with multiple positions:
    -   `after` - Add after existing content
    -   `replace` - Replace existing content
    -   `before` - Add before existing content
-   **TypeScript Support**: Full type definitions
-   **React Hooks**: `usePluginRegistry()`, `usePluginComponent()`

#### SDK API

```typescript
import {
    PluginRegistryProvider,
    Slot,
    usePluginRegistry,
} from "@broccoli/sdk/react";
import type { PluginManifest, ComponentBundle } from "@broccoli/sdk";

// Define plugin manifest
const manifest: PluginManifest = {
    name: "my-plugin",
    version: "1.0.0",
    slots: [
        {
            name: "slots.header",
            position: "append",
            component: "components/MyButton",
        },
    ],
};

// Register plugin
const { registerPlugin } = usePluginRegistry();
registerPlugin(manifest, components);
```

### Web Package (`@broccoli/web`)

The web application demonstrates:

-   **Plugin Usage**: Example plugin (`amazing-button`)
-   **ShadCN UI Integration**: Pre-configured with Tailwind CSS
-   **Slot Implementation**: Header slot with plugin injection
-   **TypeScript**: Full type safety with path aliases

## Getting Started

### Prerequisites

-   Node.js 18+ (recommended: 20+)
-   pnpm 8+

### Installation

```bash
# Install dependencies
pnpm install

# Build SDK
pnpm --filter @broccoli/sdk build

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
pnpm --filter @broccoli/sdk dev
```

### Building

```bash
# Build all packages
pnpm build

# Build specific package
pnpm --filter @broccoli/sdk build
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

2. **Define plugin manifest** (`index.ts`):

```typescript
import type { PluginManifest, ComponentBundle } from "@broccoli/sdk";
import { MyComponent } from "./components/MyComponent";

export const manifest: PluginManifest = {
    name: "my-plugin",
    version: "1.0.0",
    slots: [
        {
            name: "slots.header",
            position: "after",
            component: "components/MyComponent",
        },
    ],
};

export const components: ComponentBundle = {
    "components/MyComponent": MyComponent,
};
```

3. **Register plugin** in `App.tsx`:

```typescript
import * as MyPlugin from "./plugins/my-plugin";

function AppContent() {
    const { registerPlugin } = usePluginRegistry();

    useEffect(() => {
        registerPlugin(MyPlugin.manifest, MyPlugin.components);
    }, [registerPlugin]);
    // ...
}
```

## Technology Stack

-   **Build Tool**: Vite (Rolldown)
-   **Framework**: React 19
-   **Language**: TypeScript 5.9
-   **Styling**: Tailwind CSS 3.4
-   **UI Components**: ShadCN UI
-   **Package Manager**: pnpm
-   **Monorepo**: pnpm workspaces

## Scripts

```bash
pnpm dev      # Start development servers for all packages
pnpm build    # Build all packages
pnpm lint     # Lint all packages
pnpm test     # Run tests for all packages
```

## License

ISC
