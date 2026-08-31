import assert from "node:assert/strict";
import { describe, it } from "mocha";

async function enterText(accessibleName: string, value: string): Promise<void> {
  const input = $(`aria/${accessibleName}`);
  await input.waitForDisplayed();
  await input.setValue(value);
}

describe("ProjectHome persistence", () => {
  it("loads a created official.test project after a new app process starts", async () => {
    const projectTitle = `E2E persistence ${Date.now().toString()}`;

    await $("aria/Projects").waitForDisplayed();
    await enterText("Project title", projectTitle);
    await enterText("Planning starts", "2030-01-01T00:00:00Z");
    await enterText("Planning ends", "2030-02-01T00:00:00Z");
    await browser.execute<undefined>(
      "document.querySelector('.stacked-form').requestSubmit(); return undefined",
    );
    const projectHome = $(".project-home");
    let projectHomeText = "";
    await browser.waitUntil(
      async () => {
        projectHomeText = await projectHome.getText();
        return (
          projectHomeText.includes(projectTitle) ||
          projectHomeText.includes("Request not completed.")
        );
      },
      {
        timeout: 15_000,
        timeoutMsg: "project creation produced neither a saved project nor a request error",
      },
    );
    assert.match(projectHomeText, new RegExp(projectTitle, "u"));

    const createdProject = $(`aria/Open project ${projectTitle}`);
    await createdProject.waitForDisplayed();
    assert.match(await createdProject.getText(), /official\.test/u);

    const originalSessionId = browser.sessionId;
    await browser.reloadSession();
    assert.notStrictEqual(browser.sessionId, originalSessionId);

    await $("aria/Projects").waitForDisplayed();
    const persistedProject = $(`aria/Open project ${projectTitle}`);
    await persistedProject.waitForDisplayed();
    assert.match(await persistedProject.getText(), /official\.test/u);
  });
});
