/**
 * status-bar — minimal custom footer for pi.
 *
 * Layout:
 *   LEFT  : project path (git branch)  │  ctx %  │  5h %  7d %
 *   RIGHT : <model>  │  think <level>
 *
 * The right group is right-aligned to the terminal edge. If the line is too
 * wide, the project-path item is truncated (head-first, keeping the project
 * folder name) so model + thinking on the right are never lost.
 *
 * Sources:
 *   • project path      -> ctx.cwd (home shortened to ~)
 *   • git branch        -> footerData.getGitBranch()
 *   • context %         -> ctx.getContextUsage().percent
 *   • model / thinking  -> ctx.model / ctx.thinkingLevel
 *   • GLM 5h/7d quota   -> captured from provider response headers via the
 *                          `after_provider_response` event (pi hands extensions
 *                          every header the server sent, unfiltered)
 *
 * NOTE: as of writing, the z.ai inference API
 * (api.z.ai/api/coding/paas/v4) does NOT emit per-request quota headers for
 * the coding plan, and there is no API-key-accessible quota endpoint (the
 * 5h/7d figures are only visible in the z.ai web console, which uses
 * session-cookie auth). Until headers are observed the quota segment shows
 * "—"; the moment z.ai starts sending them (or a gateway injects them) they
 * will appear here automatically. Use `/glm-headers` to inspect what the API
 * is actually returning.
 */

import { homedir } from "node:os";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { truncateToWidth, visibleWidth } from "@earendil-works/pi-tui";

// ---------------------------------------------------------------------------
// Theme mapping helpers
// ---------------------------------------------------------------------------

/** Maps a thinking level to its pi theme foreground color key. */
const THINKING_COLOR: Record<string, string> = {
	off: "thinkingOff",
	minimal: "thinkingMinimal",
	low: "thinkingLow",
	medium: "thinkingMedium",
	high: "thinkingHigh",
	xhigh: "thinkingXhigh",
	max: "thinkingMax",
};

/** Picks a status color by "used %" (higher = worse / closer to the cap). */
function usageColor(pct: number | null): string {
	if (pct === null) return "dim";
	if (pct >= 80) return "error";
	if (pct >= 50) return "warning";
	return "muted";
}

/** Compact token formatting: 1234 -> "1.2k", 1_500_000 -> "1.50M". */
function formatTokens(n: number): string {
	if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
	if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
	return `${n}`;
}

/** Format milliseconds as a compact duration: 5s / 3m42s / 1h05m. */
function formatDuration(ms: number): string {
	const totalSec = Math.floor(ms / 1000);
	const h = Math.floor(totalSec / 3600);
	const m = Math.floor((totalSec % 3600) / 60);
	const s = totalSec % 60;
	const pad2 = (n: number) => String(n).padStart(2, "0");
	if (h > 0) return `${h}h${pad2(m)}m`;
	if (m > 0) return `${m}m${pad2(s)}s`;
	return `${s}s`;
}

/** Shorten the home-directory prefix of a path to "~". */
function shortenPath(cwd: string): string {
	const home = homedir();
	if (home && (cwd === home || cwd.startsWith(home + "\\") || cwd.startsWith(home + "/"))) {
		return "~" + cwd.slice(home.length);
	}
	return cwd;
}

/** Truncate from the head, keeping the tail (so the project folder survives). */
function truncateHead(text: string, max: number): string {
	if (max <= 0) return "";
	if (text.length <= max) return text;
	if (max === 1) return "…";
	return "…" + text.slice(text.length - (max - 1));
}

// ---------------------------------------------------------------------------
// Quota capture (module-scoped so it survives across renders / events)
// ---------------------------------------------------------------------------

interface WindowQuota {
	limit?: number;
	remaining?: number;
	used?: number;
}

interface QuotaState {
	fiveHour: WindowQuota | null;
	sevenDay: WindowQuota | null;
}

let quota: QuotaState = { fiveHour: null, sevenDay: null };
let lastStatus: number | null = null;
let lastHeaders: Record<string, string> = {};

// Cumulative time the model spent generating assistant responses this session.
// Measured per assistant message (message_start -> message_end) so tool-execution
// gaps are excluded. `activeStart` is set while a response is streaming so the
// displayed total can tick live.
let totalThinkingMs = 0;
let activeStart: number | null = null;

/** Extract a 5h/7d window's limit/remaining/used from a header set. */
function parseWindow(headers: Record<string, string>, window: "5h" | "7d"): WindowQuota | null {
	const w = window.toLowerCase();
	let limit: number | undefined;
	let remaining: number | undefined;
	let used: number | undefined;
	for (const [rawKey, rawValue] of Object.entries(headers)) {
		const key = rawKey.toLowerCase();
		if (!key.includes(w)) continue;
		const n = Number(rawValue);
		if (!Number.isFinite(n)) continue;
		if (/(remain|left)/.test(key)) remaining = n;
		else if (/(used|consumed)/.test(key)) used = n;
		else if (/(limit|max|quota|total)/.test(key)) limit = n;
	}
	if (limit === undefined && remaining === undefined && used === undefined) return null;
	return { limit, remaining, used };
}

/** Used percentage for a window, or null when it can't be derived. */
function usedPct(q: WindowQuota | null): number | null {
	if (!q) return null;
	const limit = q.limit;
	if (limit && limit > 0) {
		const used = q.used ?? (q.remaining !== undefined ? limit - q.remaining : undefined);
		if (used !== undefined) return Math.max(0, Math.min(100, (used / limit) * 100));
	}
	return null;
}

/** Human-readable quota value for a window, e.g. "12%", "8.8k", or "—". */
function formatWindow(q: WindowQuota | null): { text: string; pct: number | null } {
	const pct = usedPct(q);
	if (pct !== null) return { text: `${Math.round(pct)}%`, pct };
	if (q?.remaining !== undefined) return { text: formatTokens(q.remaining), pct: null };
	if (q?.used !== undefined) return { text: formatTokens(q.used), pct: null };
	return { text: "—", pct: null };
}

function captureHeaders(headers: Record<string, string>): void {
	lastHeaders = headers;
	const fiveHour = parseWindow(headers, "5h");
	const sevenDay = parseWindow(headers, "7d");
	if (fiveHour || sevenDay) {
		quota = {
			fiveHour: fiveHour ?? quota.fiveHour,
			sevenDay: sevenDay ?? quota.sevenDay,
		};
	}
}

// ---------------------------------------------------------------------------
// Extension
// ---------------------------------------------------------------------------

export default function (pi: ExtensionAPI) {
	// TUI handle captured from the footer factory so event handlers can ask
	// for a re-render when model / thinking / context / quota change.
	let tuiRef: { requestRender(): void } | null = null;
	const rerender = () => tuiRef?.requestRender();

	// Replace the footer (TUI only) on every session (re)start.
	pi.on("session_start", (_event, ctx) => {
		if (ctx.mode !== "tui") return;

		// Per-session reset of the thinking timer.
		totalThinkingMs = 0;
		activeStart = null;

		ctx.ui.setFooter((tui, theme, footerData) => {
			tuiRef = tui;
			const unsub = footerData.onBranchChange(() => tui.requestRender());
			return {
				dispose: () => {
					unsub();
					if (tuiRef === tui) tuiRef = null;
				},
				invalidate() {},
				render(width: number): string[] {
					const dim = (s: string) => theme.fg("dim", s);
					const sep = dim(" │ ");
					const sepW = 3; // visible width of " │ "
					const gapW = 2; // minimum gap between left and right groups

					// --- RIGHT: model + thinking level -------------------------
					const modelName = ctx.model?.name ?? ctx.model?.id ?? "no-model";
					const level = ctx.thinkingLevel ?? "off";
					const levelColor = THINKING_COLOR[level] ?? "thinkingMedium";
					const now = Date.now();
					const thinkingMs = totalThinkingMs + (activeStart !== null ? now - activeStart : 0);
					const right = [
						theme.fg("accent", modelName),
						dim("think ") + theme.fg(levelColor, level),
						dim("\u03a3 ") + theme.fg("muted", formatDuration(thinkingMs)),
					].join(sep);

					// --- LEFT (fixed): ctx % + GLM quota ------------------------
					const usage = ctx.getContextUsage();
					const pct = usage?.percent ?? null;
					const ctxText = pct !== null ? `${Math.round(pct)}%` : "—";
					const leftFixedItems = [dim("ctx ") + theme.fg(usageColor(pct), ctxText)];

					if (ctx.model?.provider === "zai") {
						const f = formatWindow(quota.fiveHour);
						const d = formatWindow(quota.sevenDay);
						leftFixedItems.push(
							dim("5h ") +
								theme.fg(usageColor(f.pct), f.text) +
								" " +
								dim("7d ") +
								theme.fg(usageColor(d.pct), d.text),
						);
					}
					const leftFixed = leftFixedItems.join(sep);

					// --- LEFT (shrinkable): project path + git branch -----------
					const branch = footerData.getGitBranch();
					let locPlain = shortenPath(ctx.cwd) + (branch ? ` (${branch})` : "");

					const rightW = visibleWidth(right);
					const leftFixedW = visibleWidth(leftFixed);
					const availForLoc = width - rightW - gapW - leftFixedW - (leftFixedW > 0 ? sepW : 0);
					if (availForLoc < locPlain.length) {
						locPlain = truncateHead(locPlain, Math.max(0, availForLoc));
					}
					const location = dim(locPlain);

					const left = locPlain.length > 0 ? location + (leftFixedW > 0 ? sep : "") + leftFixed : leftFixed;

					// Right-align the right group to the terminal edge.
					const leftW = visibleWidth(left);
					const padCount = Math.max(gapW, width - leftW - rightW);
					const line = left + " ".repeat(padCount) + right;
					return [truncateToWidth(line, width, "")];
				},
			};
		});
	});

	// Capture quota + remember raw headers from every provider response.
	pi.on("after_provider_response", (event) => {
		lastStatus = event.status;
		captureHeaders(event.headers ?? {});
		rerender();
	});

	// Track cumulative model "thinking" (generation) time, excluding tool gaps.
	// Also keep the footer in sync as live state changes.
	pi.on("message_start", (event) => {
		if (event.message?.role === "assistant") {
			if (activeStart !== null) totalThinkingMs += Date.now() - activeStart; // close dangling window
			activeStart = Date.now();
		}
		rerender(); // context also grows when a message is added
	});
	pi.on("message_update", (event) => {
		if (event.message?.role === "assistant" && activeStart !== null) rerender(); // live tick
	});
	pi.on("message_end", (event) => {
		if (event.message?.role === "assistant" && activeStart !== null) {
			totalThinkingMs += Date.now() - activeStart;
			activeStart = null;
		}
		rerender();
	});
	pi.on("model_select", () => rerender());
	pi.on("thinking_level_select", () => rerender());
	pi.on("turn_end", () => rerender());
	pi.on("session_compact", () => rerender());
	pi.on("session_shutdown", () => {
		activeStart = null; // don't carry a dangling timer across sessions
	});

	// Debug aid: inspect exactly what the GLM API is returning so the quota
	// situation can be verified. `/glm-headers off` clears the widget.
	pi.registerCommand("glm-headers", {
		description: "Show the last GLM API response headers (debug quota capture)",
		handler: async (args, ctx) => {
			if (args.trim() === "off") {
				ctx.ui.setWidget("glm-headers", undefined);
				return;
			}
			const lines: string[] = ["Last provider response headers:"];
			lines.push(`status: ${lastStatus ?? "(none yet)"}`);
			const entries = Object.entries(lastHeaders);
			if (entries.length === 0) {
				lines.push("(no headers captured yet — send a message first)");
			} else {
				for (const [k, v] of entries) lines.push(`${k}: ${v}`);
				const f = parseWindow(lastHeaders, "5h");
				const d = parseWindow(lastHeaders, "7d");
				lines.push("");
				lines.push(`parsed 5h: ${f ? JSON.stringify(f) : "(none)"}`);
				lines.push(`parsed 7d: ${d ? JSON.stringify(d) : "(none)"}`);
			}
			ctx.ui.setWidget("glm-headers", lines);
		},
	});
}
