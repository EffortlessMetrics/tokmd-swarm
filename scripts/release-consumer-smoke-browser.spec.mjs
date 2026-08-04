import { test, expect } from "@playwright/test";
import { readFileSync } from "node:fs";

const baseUrl = process.env.BASE_URL;
const zipPath = process.env.ZIP_PATH;
const expectedVersion = process.env.EXPECTED_VERSION;

if (!baseUrl || !zipPath || !expectedVersion) {
    throw new Error("BASE_URL, ZIP_PATH, and EXPECTED_VERSION are required");
}

test.setTimeout(300_000);

test("released archive-enabled WASM supports browser ZIP workflows", async ({ page }) => {
    const consoleErrors = [];
    page.on("console", (message) => {
        if (message.type() === "error") {
            consoleErrors.push(message.text());
        }
    });
    page.on("pageerror", (error) => consoleErrors.push(error.message));

    await page.goto(baseUrl, { waitUntil: "networkidle" });
    await expect(page.locator("[data-worker-capabilities]")).toContainText(expectedVersion);

    await page.locator("[data-zip-archive]").setInputFiles(zipPath);
    await expect(page.locator("[data-load-zip]")).toBeEnabled();
    await page.locator("[data-load-zip]").click();
    await expect(page.locator("[data-load-status]")).toContainText(/loaded/i, { timeout: 30_000 });

    for (const mode of ["lang", "module", "export"]) {
        await page.locator("[data-mode]").selectOption(mode);
        await page.locator("[data-result]").evaluate((element) => {
            element.textContent = "";
        });
        await page.locator("[data-run]").click();
        await expect(page.locator("[data-result]")).not.toHaveText("", { timeout: 30_000 });
        const result = JSON.parse(await page.locator("[data-result]").textContent());
        expect(result).toBeTruthy();
        expect(result.data ?? result).toBeTruthy();
    }

    await page.locator("[data-mode]").selectOption("analyze");
    await page.locator("[data-args]").fill(JSON.stringify({ preset: "receipt" }));
    await page.locator("[data-result]").evaluate((element) => {
        element.textContent = "";
    });
    await page.locator("[data-run]").click();
    await expect(page.locator("[data-result]")).not.toHaveText("", { timeout: 30_000 });
    const analysis = JSON.parse(await page.locator("[data-result]").textContent());
    expect(analysis.data ?? analysis).toBeTruthy();

    const downloadPromise = page.waitForEvent("download");
    await page.locator("[data-download]").click();
    const download = await downloadPromise;
    expect(await download.failure()).toBeNull();
    const downloadPath = await download.path();
    expect(downloadPath).toBeTruthy();
    const downloaded = JSON.parse(readFileSync(downloadPath, "utf-8"));
    expect(downloaded).toBeTruthy();
    expect(consoleErrors).toEqual([]);
});
