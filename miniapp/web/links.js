/** Paste parsing only; native Rust independently validates every destination. */
export function extractLinks(text) {
  const values = String(text).match(/https?:\/\/[^\s<>"'`，。；！？（）【】]+/gi) ?? [];
  return [...new Set(values.map(value => value.replace(/[),.;!?\]}]+$/, '')))];
}
export function withoutLinks(text, urls) {
  let result = text;
  for (const url of urls) result = result.split(url).join('');
  return result.trim();
}
