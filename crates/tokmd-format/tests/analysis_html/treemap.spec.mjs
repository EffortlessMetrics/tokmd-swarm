import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";

const template = readFileSync(new URL("../../src/analysis/templates/report.html", import.meta.url), "utf8");

async function renderChart(page, files) {
    await page.setContent(template.replace("{{REPORT_JSON}}", JSON.stringify({ files })));
    return page.locator("#treemap");
}

test("the chart sums code lines across files in each module", async ({ page }) => {
    const chart = await renderChart(page, [
        { module: "src", lang: "Rust", code: 3, lines: 11 },
        { module: "src", lang: "Rust", code: 0, lines: 7 },
        { module: "src", lang: "Rust", code: 2, lines: 5 },
        { module: "docs", lang: "Rust", code: 0, lines: 19 },
    ]);
    const cell = chart.locator(".treemap-cell");
    await expect(cell).toHaveCount(1);
    await expect(cell).toHaveText("src (5)");
    await expect(cell).toHaveAttribute("title", "src — code lines: 5");
});

test("measured zero remains explicit", async ({ page }) => {
    const chart = await renderChart(page, [{ module: "src", lang: "Rust", code: 0, lines: 7 }]);
    await expect(chart).toHaveText("0 code lines in the reported files.");
    await expect(chart.locator(".treemap-cell")).toHaveCount(0);
});

for (const [name, files] of [
    ["empty file list", []],
    ["missing code count", [{ module: "src", lang: "Rust", lines: 7 }]],
]) {
    test(name + " makes the count unavailable", async ({ page }) => {
        const chart = await renderChart(page, files);
        await expect(chart).toHaveText("Code-line counts unavailable.");
        await expect(chart.locator(".treemap-cell")).toHaveCount(0);
    });
}
