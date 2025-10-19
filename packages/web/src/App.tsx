import { useEffect } from "react";
import {
    PluginRegistryProvider,
    Slot,
    usePluginRegistry,
} from "@broccoli/sdk/react";
import { Button } from "@/components/ui/button";
import "./App.css";

// Import plugins
import * as AmazingButtonPlugin from "./plugins/amazing-button";

function AppContent() {
    const { registerPlugin } = usePluginRegistry();

    useEffect(() => {
        // Register plugins on mount
        registerPlugin(
            AmazingButtonPlugin.manifest,
            AmazingButtonPlugin.components
        );
    }, [registerPlugin]);

    return (
        <div className="min-h-screen bg-background p-8">
            <div className="max-w-4xl mx-auto space-y-8">
                <header className="border-b pb-4">
                    <h1 className="text-4xl font-bold mb-4">🥦 Broccoli</h1>
                    <p className="text-muted-foreground">
                        Plugin System with Slot Architecture
                    </p>
                </header>

                {/* Slot Example: Header */}
                <Slot name="slots.header" className="flex gap-4 flex-wrap">
                    <Button variant="outline">Default Header Button</Button>
                </Slot>
            </div>
        </div>
    );
}

function App() {
    return (
        <PluginRegistryProvider>
            <AppContent />
        </PluginRegistryProvider>
    );
}

export default App;
