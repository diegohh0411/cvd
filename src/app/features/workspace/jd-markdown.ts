import { marked } from 'marked';

/**
 * Renders a pasted or fetched job description as HTML. Single newlines are
 * kept as line breaks because most postings are plain text with hard wraps.
 * The result is bound with [innerHTML], which Angular sanitizes.
 */
export function renderJd(text: string): string {
  return marked.parse(text, { async: false, breaks: true, gfm: true });
}
