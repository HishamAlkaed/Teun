import { describe, it, expect } from "vitest";
import { SettingsProvider, useSettings } from "./useSettings";
import { createElement } from "react";
import { renderToString } from "react-dom/server";

describe("useSettings defaults", () => {
  it("returns default settings on first render", () => {
    let capturedSettings: unknown;

    function TestComponent() {
      const { settings } = useSettings();
      capturedSettings = settings;
      return null;
    }

    renderToString(
      createElement(SettingsProvider, null, createElement(TestComponent)),
    );

    expect(capturedSettings).toEqual({
      judgeThreshold: 70,
      judgeLowThreshold: 40,
      language: "nl",
    });
  });

  it("useSettings throws when used outside provider", () => {
    function BadComponent() {
      useSettings();
      return null;
    }

    expect(() => renderToString(createElement(BadComponent))).toThrow(
      "useSettings must be used within SettingsProvider",
    );
  });
});
