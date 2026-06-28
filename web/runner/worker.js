import {
    createErrorMessage,
    createReadyMessage,
    normalizeAnalyzePreset,
    SUPPORTED_ANALYZE_PRESETS,
    SUPPORTED_MODES,
} from "./messages.js";
import { handleRunnerMessage } from "./runtime.js";

let emitMessage;
let subscribe;
let nodeWorkerData = null;
let isNodeWorker = false;
const DEFAULT_WASM_MODULE_URL = new URL("./vendor/tokmd-wasm/tokmd_wasm.js", import.meta.url);
const DEFAULT_WASM_BINARY_URL = new URL("./vendor/tokmd-wasm/tokmd_wasm_bg.wasm", import.meta.url);
const MODE_EXPORTS = Object.freeze({
    lang: "runLang",
    module: "runModule",
    export: "runExport",
    analyze: "runAnalyze",
});

if (
    typeof globalThis.postMessage === "function" &&
    typeof globalThis.addEventListener === "function"
) {
    emitMessage = (message) => globalThis.postMessage(message);
    subscribe = (handler) => {
        globalThis.addEventListener("message", (event) => {
            handler(event.data);
        });
    };
} else {
    const { parentPort, workerData } = await import("node:worker_threads");
    isNodeWorker = true;
    nodeWorkerData = workerData;

    emitMessage = (message) => parentPort.postMessage(message);
    subscribe = (handler) => {
        parentPort.on("message", handler);
    };
}

function resolveRunnerInputs(args) {
    return args.inputs ?? args.scan?.inputs ?? [];
}

function createStubRunner() {
    const supportedModes = [...SUPPORTED_MODES];

    return {
        runLang(args) {
            const inputs = resolveRunnerInputs(args);
            return {
                mode: "lang",
                scan: {
                    paths: inputs.map((input) => input.path),
                },
                total: {
                    files: inputs.length,
                },
            };
        },
        runModule(args) {
            const inputs = resolveRunnerInputs(args);
            return {
                mode: "module",
                rows: inputs.map((input) => ({ module: input.path })),
            };
        },
        runExport(args) {
            const inputs = resolveRunnerInputs(args);
            return {
                mode: "export",
                rows: inputs.map((input) => ({ path: input.path })),
            };
        },
        runAnalyze(args) {
            const inputs = resolveRunnerInputs(args);
            return {
                mode: "analysis",
                preset: normalizeAnalyzePreset(args),
                source: {
                    inputs: inputs.map((input) => input.path),
                },
            };
        },
        runJsonBytes(mode, args, archiveBytes) {
            return {
                mode,
                archiveBytes: archiveBytes.length,
                options: args,
            };
        },
        engine: {
            version: "stub",
            schemaVersion: 0,
            analysisSchemaVersion: 0,
        },
        capabilities: {
            modes: supportedModes,
            analyzePresets: [...SUPPORTED_ANALYZE_PRESETS],
            missingExports: [],
            zipball: true,
        },
    };
}

function describeMissingExports(wasmModule) {
    const missing = [];

    const requiredFunctions = {
        default: "default",
        version: "version",
        schemaVersion: "schemaVersion",
    };

    for (const [key, symbol] of Object.entries(requiredFunctions)) {
        if (typeof wasmModule[key] !== "function") {
            missing.push(symbol);
        }
    }

    return missing;
}

function uniqueSupportedStrings(values, allowedValues) {
    const allowed = new Set(allowedValues);
    const seen = new Set();
    const result = [];

    for (const value of Array.isArray(values) ? values : []) {
        if (typeof value !== "string" || !allowed.has(value) || seen.has(value)) {
            continue;
        }

        seen.add(value);
        result.push(value);
    }

    return result;
}

function buildExportCapabilities(wasmModule) {
    const modes = Object.entries(MODE_EXPORTS)
        .filter(([, exportName]) => typeof wasmModule[exportName] === "function")
        .map(([mode]) => mode);

    return {
        modes,
        analyzePresets: typeof wasmModule.runAnalyze === "function" ? [...SUPPORTED_ANALYZE_PRESETS] : [],
    };
}

function readWasmCapabilityPayload(wasmModule) {
    if (typeof wasmModule.capabilities !== "function") {
        return null;
    }

    let capabilities;
    try {
        capabilities = wasmModule.capabilities();
    } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        throw new Error(`tokmd-wasm capabilities export failed: ${message}`);
    }

    if (!capabilities || typeof capabilities !== "object" || Array.isArray(capabilities)) {
        throw new Error("tokmd-wasm capabilities export returned an invalid payload");
    }

    return capabilities;
}

function buildModeCapabilities(wasmModule) {
    const exportCapabilities = buildExportCapabilities(wasmModule);
    const wasmCapabilities = readWasmCapabilityPayload(wasmModule);

    if (!wasmCapabilities) {
        return exportCapabilities;
    }

    const exportedModes = new Set(exportCapabilities.modes);
    const modes = uniqueSupportedStrings(
        wasmCapabilities.modes,
        Object.keys(MODE_EXPORTS)
    ).filter((mode) => exportedModes.has(mode));
    const analyzePresets = modes.includes("analyze")
        ? uniqueSupportedStrings(
              wasmCapabilities.analyze?.rootlessPresets,
              SUPPORTED_ANALYZE_PRESETS
          )
        : [];

    return {
        modes,
        analyzePresets,
    };
}

function createModeHandler(wasmModule, exportName, label) {
    if (typeof wasmModule[exportName] === "function") {
        return (args) => wasmModule[exportName](args);
    }

    return () => {
        throw new Error(`tokmd-wasm bundle does not provide ${label}`);
    };
}

function createMissingExportsError(missingExports) {
    const missing = missingExports.join(", ");

    return new Error(`tokmd-wasm bundle is missing required exports: ${missing}`);
}

function parseTokmdEnvelope(envelopeJson) {
    let envelope;

    try {
        envelope = JSON.parse(envelopeJson);
    } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        throw new Error(`tokmd-wasm returned invalid JSON envelope: ${message}`);
    }

    if (!envelope || typeof envelope !== "object") {
        throw new Error("tokmd-wasm returned an invalid envelope payload");
    }

    if (envelope.ok !== true) {
        const code =
            typeof envelope.error?.code === "string" ? envelope.error.code : "run_failed";
        const message =
            typeof envelope.error?.message === "string"
                ? envelope.error.message
                : "tokmd archive byte run failed";
        const error = new Error(message);
        error.code = code;
        throw error;
    }

    return envelope.data;
}

function createJsonBytesHandler(wasmModule) {
    if (typeof wasmModule.runJsonBytes !== "function") {
        return null;
    }

    return (mode, args, archiveBytes) => {
        const envelopeJson = wasmModule.runJsonBytes(
            mode,
            JSON.stringify(args),
            archiveBytes
        );
        return parseTokmdEnvelope(envelopeJson);
    };
}

function createRunnerFromWasmModule(wasmModule) {
    const missingExports = describeMissingExports(wasmModule);
    if (missingExports.length > 0) {
        throw createMissingExportsError(missingExports);
    }

    const capabilities = {
        ...buildModeCapabilities(wasmModule),
        missingExports,
        zipball: typeof wasmModule.runJsonBytes === "function",
    };

    const runJsonBytes = createJsonBytesHandler(wasmModule);

    return {
        runLang: createModeHandler(wasmModule, "runLang", "lang mode"),
        runModule: createModeHandler(wasmModule, "runModule", "module mode"),
        runExport: createModeHandler(wasmModule, "runExport", "export mode"),
        runAnalyze: createModeHandler(wasmModule, "runAnalyze", "analyze mode"),
        ...(runJsonBytes ? { runJsonBytes } : {}),
        capabilities,
        engine: {
            version: wasmModule.version(),
            schemaVersion: wasmModule.schemaVersion(),
            analysisSchemaVersion:
                typeof wasmModule.analysisSchemaVersion === "function"
                    ? wasmModule.analysisSchemaVersion()
                    : null,
        },
    };
}

function resolveWasmModuleUrl() {
    if (typeof nodeWorkerData?.wasmModuleUrl === "string" && nodeWorkerData.wasmModuleUrl.trim()) {
        return nodeWorkerData.wasmModuleUrl;
    }

    return DEFAULT_WASM_MODULE_URL.href;
}

function resolveWasmBinaryPath() {
    if (typeof nodeWorkerData?.wasmBinaryPath === "string" && nodeWorkerData.wasmBinaryPath.trim()) {
        return nodeWorkerData.wasmBinaryPath;
    }

    return DEFAULT_WASM_BINARY_URL;
}

async function loadTokmdRunner() {
    if (nodeWorkerData?.runnerMode === "stub") {
        return createStubRunner();
    }

    const moduleUrl = resolveWasmModuleUrl();
    const wasmModule = await import(moduleUrl);
    const missingExports = describeMissingExports(wasmModule);
    if (missingExports.length > 0) {
        throw createMissingExportsError(missingExports);
    }

    const modeCapabilities = buildExportCapabilities(wasmModule);
    const hasAnyModes = modeCapabilities.modes.length > 0;

    if (!hasAnyModes) {
        throw new Error("tokmd-wasm bundle exposes no supported run modes");
    }

    if (isNodeWorker) {
        const { readFile } = await import("node:fs/promises");
        const wasmPath = resolveWasmBinaryPath();
        await wasmModule.default({ module_or_path: await readFile(wasmPath) });
    } else {
        await wasmModule.default();
    }

    return createRunnerFromWasmModule(wasmModule);
}

let runner = null;
let bootError = null;

const runnerReady = loadTokmdRunner()
    .then((loadedRunner) => {
        runner = loadedRunner;
        emitMessage(
            createReadyMessage({
                capabilities: {
                    wasm: true,
                    downloads: true,
                    progress: true,
                    zipball: Boolean(loadedRunner.runJsonBytes),
                    modes: loadedRunner.capabilities.modes,
                    analyzePresets: loadedRunner.capabilities.analyzePresets,
                },
                engine: loadedRunner.engine,
            })
        );
        return loadedRunner;
    })
    .catch((error) => {
        bootError = error;
        emitMessage(
            createErrorMessage(
                null,
                "wasm_boot_failed",
                `browser runner failed to initialize tokmd-wasm: ${error instanceof Error ? error.message : String(error)}`
            )
        );
        return null;
    });

subscribe((message) => {
    void runnerReady.then(async () => {
        emitMessage(await handleRunnerMessage(message, {
            runner,
            runnerCapabilities: runner?.capabilities ?? {},
            bootError,
            onProgress: emitMessage,
        }));
    });
});
