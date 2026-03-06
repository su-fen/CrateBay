# CrateBay UI Design System

> **Version:** 0.8.0 preview · **Last updated:** 2026-03-06

This document defines the visual language and component specifications for the CrateBay GUI.

## Design Tokens

| Token | Value | Description |
|-------|-------|-------------|
| Base font size | 13px | Body text |
| Border radius sm | 6px | Inline elements, badges |
| Border radius md | 8px | Buttons, inputs |
| Border radius lg | 10px | Cards |
| Border radius xl | 12px | Panels, modals |
| Spacing unit | 4px | Multiples: 4 / 8 / 12 / 16 / 20 / 24 |
| Icon size | 16×16 | Unified SVG size |
| SVG stroke-width | 2 | Unified stroke width |

## Color Palette

### Dark Theme (default)

| Token | Value | Usage |
|-------|-------|-------|
| `--bg` | `#0f111a` | App background |
| `--surface` | `#1a1d2e` | Card / sidebar background |
| `--surface2` | `#232640` | Secondary surfaces |
| `--border` | `#2a2d45` | Borders |
| `--text` | `#e2e8f0` | Primary text |
| `--text2` | `#94a3b8` | Secondary text |
| `--text3` | `#4b5277` | Muted text |
| `--purple` | `#8b5cf6` | Brand primary |
| `--purple-hover` | `#6d28d9` | Brand hover |
| `--cyan` | `#22d3ee` | Accent |
| `--green` | `#34d399` | Running / success |
| `--red` | `#f87171` | Stopped / danger |

### Light Theme

Overrides applied via `.app.light` class — see `App.css` for full values.

## Buttons

| Class | Height | Padding | Font | Radius | Usage |
|-------|--------|---------|------|--------|-------|
| `.btn` | 32px | 6px 12px | 12px / 600 | 8px | Toolbar / default |
| `.btn.primary` | 32px | 6px 12px | 12px / 600 | 8px | Primary action |
| `.btn.sm` | 28px | 4px 10px | 11px / 600 | 6px | Card / inline |
| `.btn.xs` | 24px | 2px 8px | 11px / 600 | 6px | Compact |
| `.icon-btn` | 32×32 | centered | — | 8px | Standalone icon button |
| `.action-btn` | 28×28 | centered | — | 6px | List row action |

## Inputs

| Class | Height | Padding | Font | Radius |
|-------|--------|---------|------|--------|
| `.input` | 32px | 6px 10px | 12px | 8px |
| `.select` | 32px | 6px 10px | 12px | 8px |

## Modals

| Property | Value |
|----------|-------|
| Form modal max-width | 480px |
| Content modal max-width | 720px |
| Border radius | 12px |
| Header height | 48px |
| Footer | Buttons right-aligned |

## Icons

- All SVG icons use `stroke-width: 2`, `stroke-linecap: round`, `stroke-linejoin: round`.
- Standard icon size in nav: 18×18 within a 20×20 container.
- Standard icon size in buttons: 14×14.

## Layout

| Component | Width |
|-----------|-------|
| Sidebar | 220px (collapses to 56px on mobile) |
| Top bar | 56px height |
| Content padding | 20px 24px |
| Card gap | 8–12px |
| Settings max-width | 640px |
