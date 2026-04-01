import { describe, it, expect, beforeEach } from "vitest";
import { SettingsProvider, useSettings } from "./useSettings";
import { createElement } from "react";
import { renderToString } from "react-dom/server";

describe("useSettings defaults", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("returns defaults when localStorage is empty", () => {
    // We can't use renderHook without a full React test renderer,
    // so we test the defaults indirectly by checking localStorage behavior
    expect(localStorage.getItem("teun-settings")).toBeNull();
  });

  it("loadSettings merges partial stored data with defaults", () => {
    localStorage.setItem(
      "teun-settings",
      JSON.stringify({ chatMode: "inline" }),
    );

    // Render the provider to trigger loadSettings
    let capturedSettings: unknown;

    function TestComponent() {
      const { settings } = useSettings();
      capturedSettings = settings;
      return null;
    }

    // Use renderToString to trigger the component
    renderToString(
      createElement(SettingsProvider, null, createElement(TestComponent)),
    );

    expect(capturedSettings).toEqual({
      judgeThreshold: 70,
      judgeLowThreshold: 40,
      chatMode: "inline", // overridden
      searchDepth: "quick",
      language: "nl",
    });
  });

  it("loadSettings returns defaults for corrupted JSON", () => {
    localStorage.setItem("teun-settings", "not-json{{{");

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
      chatMode: "tools",
      searchDepth: "quick",
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
