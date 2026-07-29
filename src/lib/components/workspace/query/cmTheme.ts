/**
 * CodeMirror theme + highlight styles bound to Mystique design tokens
 * so light/dark themes stay consistent without a separate CM theme package.
 */
import { EditorView } from "@codemirror/view";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";

/** Syntax colors → CSS variables from tokens.css */
export const mystiqueHighlight = syntaxHighlighting(
  HighlightStyle.define([
    { tag: t.keyword, color: "var(--accent)", fontWeight: "600" },
    { tag: t.moduleKeyword, color: "var(--accent)", fontWeight: "600" },
    { tag: t.controlKeyword, color: "var(--accent)", fontWeight: "600" },
    { tag: t.operatorKeyword, color: "var(--conn-cyan)" },
    { tag: t.definitionKeyword, color: "var(--accent)", fontWeight: "600" },
    { tag: t.bool, color: "var(--conn-violet)" },
    { tag: t.null, color: "var(--conn-violet)" },
    { tag: t.string, color: "var(--status-success)" },
    { tag: t.special(t.string), color: "var(--status-success)" },
    { tag: t.number, color: "var(--status-warning)" },
    { tag: t.integer, color: "var(--status-warning)" },
    { tag: t.float, color: "var(--status-warning)" },
    { tag: t.comment, color: "var(--text-muted)", fontStyle: "italic" },
    { tag: t.lineComment, color: "var(--text-muted)", fontStyle: "italic" },
    { tag: t.blockComment, color: "var(--text-muted)", fontStyle: "italic" },
    { tag: t.operator, color: "var(--text-secondary)" },
    { tag: t.punctuation, color: "var(--text-secondary)" },
    { tag: t.bracket, color: "var(--text-secondary)" },
    { tag: t.paren, color: "var(--text-secondary)" },
    { tag: t.squareBracket, color: "var(--text-secondary)" },
    { tag: t.brace, color: "var(--text-secondary)" },
    { tag: t.name, color: "var(--text-primary)" },
    { tag: t.variableName, color: "var(--text-primary)" },
    { tag: t.propertyName, color: "var(--conn-teal)" },
    { tag: t.attributeName, color: "var(--conn-teal)" },
    { tag: t.typeName, color: "var(--conn-cyan)" },
    { tag: t.className, color: "var(--conn-cyan)" },
    { tag: t.namespace, color: "var(--conn-indigo)" },
    { tag: t.function(t.variableName), color: "var(--conn-amber)" },
    { tag: t.function(t.propertyName), color: "var(--conn-amber)" },
    { tag: t.meta, color: "var(--text-muted)" },
    { tag: t.invalid, color: "var(--status-danger)" },
  ]),
);

/** Editor chrome: fills parent, mono body type, token surfaces. */
export const mystiqueEditorTheme = EditorView.theme(
  {
    "&": {
      height: "100%",
      fontSize: "0.8125rem" /* text-body */,
      backgroundColor: "var(--bg-panel)",
      color: "var(--text-primary)",
    },
    "&.cm-focused": {
      outline: "none",
    },
    ".cm-scroller": {
      fontFamily: "var(--mds-font-mono)",
      lineHeight: "1.625",
      overflow: "auto",
    },
    ".cm-content": {
      caretColor: "var(--accent)",
      /* Tight left padding so text sits close to line numbers */
      padding: "0.5rem 0.75rem 0.5rem 0.35rem",
      minHeight: "100%",
    },
    ".cm-line": {
      padding: "0 1px",
    },
    ".cm-cursor, .cm-dropCursor": {
      borderLeftColor: "var(--accent)",
    },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection":
      {
        backgroundColor: "var(--bg-selected) !important",
      },
    ".cm-activeLine": {
      backgroundColor: "var(--bg-hover)",
    },
    ".cm-gutters": {
      backgroundColor: "var(--bg-elevated)",
      color: "var(--text-muted)",
      border: "none",
      borderRight: "1px solid var(--border-subtle)",
      minWidth: "1.75rem",
    },
    ".cm-gutterElement": {
      /* Less space between the number and the text */
      padding: "0 0.25rem 0 0.2rem",
      minWidth: "1.5rem",
    },
    ".cm-lineNumbers .cm-gutterElement": {
      textAlign: "right",
      paddingRight: "0.25rem",
    },
    ".cm-activeLineGutter": {
      backgroundColor: "var(--bg-hover)",
      color: "var(--text-secondary)",
    },
    ".cm-foldPlaceholder": {
      backgroundColor: "var(--bg-input)",
      border: "none",
      color: "var(--text-muted)",
    },
    ".cm-tooltip": {
      backgroundColor: "var(--bg-elevated)",
      border: "1px solid var(--border-subtle)",
      color: "var(--text-primary)",
    },
    ".cm-panels": {
      backgroundColor: "var(--bg-elevated)",
      color: "var(--text-primary)",
    },
    ".cm-placeholder": {
      color: "var(--text-muted)",
      fontStyle: "normal",
    },
  },
  { dark: true },
);
