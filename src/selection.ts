export interface Point {
  x: number;
  y: number;
}
export function pixelRect(
  start: Point,
  end: Point,
  viewport: { width: number; height: number },
  image: { width: number; height: number },
) {
  if (viewport.width <= 0 || viewport.height <= 0) return null;
  const clamp = (value: number, limit: number) =>
    Math.max(0, Math.min(value, limit));
  const x = clamp(
    Math.floor((Math.min(start.x, end.x) * image.width) / viewport.width),
    image.width,
  );
  const y = clamp(
    Math.floor((Math.min(start.y, end.y) * image.height) / viewport.height),
    image.height,
  );
  const right = clamp(
    Math.ceil((Math.max(start.x, end.x) * image.width) / viewport.width),
    image.width,
  );
  const bottom = clamp(
    Math.ceil((Math.max(start.y, end.y) * image.height) / viewport.height),
    image.height,
  );
  if (right - x < 4 || bottom - y < 4) return null;
  return { x, y, width: right - x, height: bottom - y };
}
