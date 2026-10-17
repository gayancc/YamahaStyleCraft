import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EffectBrowser } from "./EffectBrowser";
import { useStudio } from "./store";
import type { View } from "./contracts";
const view = { name: "Effects", revision: 3, notes: [] } as unknown as View;
const header = {
  boundary: 100,
  editProblem: null,
  reverb: { msb: 1, lsb: 0, returnLevel: 64, lateChanges: false },
  chorus: { msb: null, lsb: null, returnLevel: null, lateChanges: true },
};

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), open: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));

describe("EffectBrowser", () => {
  afterEach(() => cleanup());
  beforeEach(() => {
    cleanup();
    useStudio.setState({ view: null, busy: false });
    mocks.invoke.mockReset();
    mocks.open.mockReset();
    mocks.invoke.mockImplementation(
      async (cmd: string, args?: { block?: string }) => {
        if (cmd === "inspect_style_effects") return header;
        if (cmd === "apply_style_effects") return { ...view, revision: 4 };
        if (cmd === "effect_catalog_status") {
          return {
            edition: "Genos v2.00",
            sourceName: "genos_en_dl_f0.pdf",
            sourceSha256: "abc",
            count: 528,
            reverb: 60,
            chorus: 108,
            variation: 360,
          };
        }
        if (cmd === "search_effect_catalog") {
          const block = args?.block ?? "reverb";
          return {
            total: 1,
            effects: [
              {
                block: block || "reverb",
                category: "Reverb",
                name: "Real Large Hall",
                description: "Real reverb",
                msb: 1,
                lsb: 32,
                parameterGroup: "REAL REVERB",
                page: 46,
              },
            ],
          };
        }
        if (cmd === "apply_genos_system_effect") {
          return { block: "reverb", packetCount: 1, packets: [[0xf0]] };
        }
        return null;
      },
    );
  });

  it("lists effect types and applies reverb SysEx when MIDI out is set", async () => {
    render(
      <EffectBrowser
        output={{ index: 1, name: "Digital Workstation-1" }}
        onClose={vi.fn()}
      />,
    );
    await screen.findByText("Real Large Hall");
    expect(screen.getByText(/reverb 60/i)).toBeTruthy();
    fireEvent.click(screen.getByRole("option", { name: /Real Large Hall/i }));
    await waitFor(() =>
      expect(screen.getByLabelText("Selected effect type").textContent).toMatch(
        /MSB 1/,
      ),
    );
    fireEvent.click(screen.getByRole("button", { name: /Apply to Genos/i }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("apply_genos_system_effect", {
        index: 1,
        name: "Digital Workstation-1",
        block: "reverb",
        msb: 1,
        lsb: 32,
        returnLevel: null,
      }),
    );
    expect(await screen.findByText(/Sent 1 SysEx packet/i)).toBeTruthy();
  });

  it("disables apply without MIDI output", async () => {
    render(<EffectBrowser output={null} onClose={vi.fn()} />);
    await screen.findByText("Real Large Hall");
    fireEvent.click(screen.getByRole("option", { name: /Real Large Hall/i }));
    expect(
      (
        screen.getByRole("button", {
          name: /Apply to Genos/i,
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(screen.getByText(/Select a MIDI output/i)).toBeTruthy();
  });

  it("clears an old selected effect when switching blocks", async () => {
    render(
      <EffectBrowser output={{ index: 1, name: "Genos" }} onClose={vi.fn()} />,
    );
    fireEvent.click(
      await screen.findByRole("option", { name: /Real Large Hall/i }),
    );
    fireEvent.change(screen.getByLabelText("Effect block"), {
      target: { value: "variation" },
    });
    await screen.findByRole("option", { name: /variation/i });
    expect(
      (
        screen.getByRole("button", {
          name: /Apply to Genos/i,
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(screen.queryByLabelText("Selected effect type")).toBeNull();
  });

  it("imports independently, preserves existing catalog on error, and sends nothing", async () => {
    render(<EffectBrowser output={null} onClose={vi.fn()} />);
    await screen.findByText("Real Large Hall");
    mocks.open.mockResolvedValue("bad.pdf");
    const previous = mocks.invoke.getMockImplementation()!;
    mocks.invoke.mockImplementation((cmd: string, args: unknown) =>
      cmd === "import_effect_catalog"
        ? Promise.reject("Wrong PDF edition")
        : previous(cmd, args),
    );
    fireEvent.click(
      screen.getByRole("button", { name: /Import effect Data List/i }),
    );
    await screen.findByText("Wrong PDF edition");
    expect(screen.getByText("Real Large Hall")).toBeTruthy();
    expect(
      mocks.invoke.mock.calls.some(
        ([cmd]) =>
          cmd === "apply_genos_system_effect" || cmd === "import_voice_catalog",
      ),
    ).toBe(false);
  });

  it("saves a type and return without an output, in one file transaction and no live call", async () => {
    useStudio.setState({ view });
    const close = vi.fn();
    render(<EffectBrowser output={null} onClose={close} />);
    await screen.findByLabelText("Initial Style effect");
    expect(screen.getByText(/Type MSB 1 · LSB 0/)).toBeTruthy();
    fireEvent.click(
      await screen.findByRole("option", { name: /Real Large Hall/ }),
    );
    fireEvent.change(screen.getByLabelText("System effect return level"), {
      target: { value: "100" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Apply to Style" }));
    await waitFor(() => expect(close).toHaveBeenCalledOnce());
    expect(mocks.invoke).toHaveBeenCalledWith("apply_style_effects", {
      revision: 3,
      changes: [
        { block: "reverb", effect: { msb: 1, lsb: 32 }, returnLevel: 100 },
      ],
    });
    expect(
      mocks.invoke.mock.calls.some(
        ([cmd]) => cmd === "apply_genos_system_effect",
      ),
    ).toBe(false);
    expect(useStudio.getState().view?.revision).toBe(4);
    expect(useStudio.getState().busy).toBe(false);
  });

  it("allows return alone without a catalog and leaves unspecified types unknown", async () => {
    useStudio.setState({ view });
    const previous = mocks.invoke.getMockImplementation()!;
    mocks.invoke.mockImplementation((cmd, args) =>
      cmd === "effect_catalog_status"
        ? Promise.resolve(null)
        : previous(cmd, args),
    );
    render(<EffectBrowser output={null} onClose={vi.fn()} />);
    await screen.findByLabelText("Initial Style effect");
    fireEvent.change(screen.getByLabelText("Effect block"), {
      target: { value: "chorus" },
    });
    expect(
      screen.getByText(/Type unspecified · Return unspecified/),
    ).toBeTruthy();
    expect(screen.getByText(/Later effect messages/)).toBeTruthy();
    expect(
      (
        screen.getByRole("button", {
          name: "Apply to Style",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    fireEvent.change(screen.getByLabelText("System effect return level"), {
      target: { value: "0" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Apply to Style" }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("apply_style_effects", {
        revision: 3,
        changes: [{ block: "chorus", effect: null, returnLevel: 0 }],
      }),
    );
  });

  it("rejects invalid returns, clears block-specific drafts, and disables variation authoring", async () => {
    useStudio.setState({ view });
    render(<EffectBrowser output={null} onClose={vi.fn()} />);
    fireEvent.click(
      await screen.findByRole("option", { name: /Real Large Hall/ }),
    );
    const input = screen.getByLabelText("System effect return level");
    for (const value of ["128", "-1", "1.5", "x"]) {
      fireEvent.change(input, { target: { value } });
      expect(
        (
          screen.getByRole("button", {
            name: "Apply to Style",
          }) as HTMLButtonElement
        ).disabled,
      ).toBe(true);
    }
    fireEvent.change(input, { target: { value: "90" } });
    fireEvent.change(screen.getByLabelText("Effect block"), {
      target: { value: "variation" },
    });
    expect((input as HTMLInputElement).value).toBe("");
    expect(screen.queryByLabelText("Selected effect type")).toBeNull();
    expect(
      (
        screen.getByRole("button", {
          name: "Apply to Style",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
  });

  it("blocks file edits for a replaced document even with the same revision", async () => {
    useStudio.setState({ view });
    render(<EffectBrowser output={null} onClose={vi.fn()} />);
    fireEvent.click(
      await screen.findByRole("option", { name: /Real Large Hall/ }),
    );
    act(() => useStudio.setState({ view: { ...view } }));
    expect(screen.getByText(/Document changed. Close/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Apply to Style" }));
    expect(
      mocks.invoke.mock.calls.some(([cmd]) => cmd === "apply_style_effects"),
    ).toBe(false);
  });

  it("keeps live application available for a read-only Style and cancel edits nothing", async () => {
    useStudio.setState({ view });
    const previous = mocks.invoke.getMockImplementation()!;
    mocks.invoke.mockImplementation((cmd, args) =>
      cmd === "inspect_style_effects"
        ? Promise.resolve({
            ...header,
            editProblem: "Unsupported source header",
          })
        : previous(cmd, args),
    );
    const close = vi.fn();
    render(
      <EffectBrowser output={{ index: 0, name: "Genos" }} onClose={close} />,
    );
    await screen.findByText(/Unsupported source header/);
    fireEvent.click(
      await screen.findByRole("option", { name: /Real Large Hall/ }),
    );
    expect(
      (
        screen.getByRole("button", {
          name: "Apply to Style",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(
      (
        screen.getByRole("button", {
          name: "Apply to Genos",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false);
    fireEvent.click(screen.getAllByRole("button", { name: "Close" })[0]);
    expect(close).toHaveBeenCalledOnce();
    expect(
      mocks.invoke.mock.calls.some(([cmd]) => cmd.startsWith("apply_")),
    ).toBe(false);
  });

  it("guards duplicate apply, Escape and close while pending; errors retain the draft for retry", async () => {
    useStudio.setState({ view });
    let reject!: (e: string) => void;
    const previous = mocks.invoke.getMockImplementation()!;
    mocks.invoke.mockImplementation((cmd, args) =>
      cmd === "apply_style_effects"
        ? new Promise((_, r) => {
            reject = r;
          })
        : previous(cmd, args),
    );
    const close = vi.fn();
    render(<EffectBrowser output={null} onClose={close} />);
    fireEvent.click(
      await screen.findByRole("option", { name: /Real Large Hall/ }),
    );
    fireEvent.change(screen.getByLabelText("System effect return level"), {
      target: { value: "70" },
    });
    const apply = screen.getByRole("button", { name: "Apply to Style" });
    fireEvent.click(apply);
    fireEvent.click(apply);
    fireEvent.keyDown(window, { key: "Escape" });
    fireEvent.click(screen.getAllByRole("button", { name: "Close" })[0]);
    expect(close).not.toHaveBeenCalled();
    expect(
      mocks.invoke.mock.calls.filter(([cmd]) => cmd === "apply_style_effects"),
    ).toHaveLength(1);
    await act(async () => reject("Header changed"));
    await screen.findByText("Header changed");
    expect(
      (screen.getByLabelText("System effect return level") as HTMLInputElement)
        .value,
    ).toBe("70");
    expect((apply as HTMLButtonElement).disabled).toBe(false);
    expect(useStudio.getState().busy).toBe(false);
  });

  it("reports an exact no-op returned by the core", async () => {
    useStudio.setState({ view });
    const previous = mocks.invoke.getMockImplementation()!;
    mocks.invoke.mockImplementation((cmd, args) =>
      cmd === "apply_style_effects"
        ? Promise.resolve(view)
        : previous(cmd, args),
    );
    const close = vi.fn();
    render(<EffectBrowser output={null} onClose={close} />);
    await screen.findByLabelText("Initial Style effect");
    fireEvent.change(screen.getByLabelText("System effect return level"), {
      target: { value: "64" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Apply to Style" }));
    await waitFor(() => expect(close).toHaveBeenCalledOnce());
    expect(useStudio.getState().message).toBe("Style effect unchanged");
  });
});
