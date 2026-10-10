import JSZip from "jszip";

export interface PracticeSentence {
  text: string;
  startMs: number | null;
  endMs: number | null;
}
export interface PracticeDocument {
  id: string;
  title: string;
  kind: string;
  chapters: { title: string; sentences: PracticeSentence[] }[];
  audioFile: string | null;
}
export interface PracticeProgress {
  chapterIndex: number;
  sentenceIndex: number;
  copy: string;
  speed: number;
  audioMode: "original" | "voice";
  voiceId: string;
}
export interface PracticeState extends PracticeProgress {
  document: PracticeDocument;
}
export function practiceProgress(state: PracticeState): PracticeProgress {
  return {
    chapterIndex: state.chapterIndex,
    sentenceIndex: state.sentenceIndex,
    copy: state.copy,
    speed: state.speed,
    audioMode: state.audioMode,
    voiceId: state.voiceId,
  };
}
export const normalizeCopy = (text: string) =>
  text
    .normalize("NFKC")
    .replace(/[‘’]/g, "'")
    .replace(/[“”]/g, '"')
    .replace(/\s+/g, " ")
    .trim();

export function compareCopy(text: string, copy: string) {
  const expected = Array.from(normalizeCopy(text));
  const typed = Array.from(normalizeCopy(copy));
  const wrong = typed.findIndex(
    (char, index) => char.toLowerCase() !== expected[index]?.toLowerCase(),
  );
  return {
    complete:
      typed.length > 0 && wrong === -1 && typed.length === expected.length,
    wrong,
    characters: Array.from(
      { length: Math.max(expected.length, typed.length) },
      (_, index) => ({
        text: typed[index] ?? expected[index],
        state:
          index >= typed.length
            ? "remaining"
            : typed[index].toLowerCase() === expected[index]?.toLowerCase()
              ? "correct"
              : "incorrect",
      }),
    ),
  };
}

export function highlightCopy(text: string, copy: string) {
  const expected = Array.from(normalizeCopy(text));
  const raw = Array.from(copy);
  let position = 0;
  let pendingSpace = false;
  let lastLetter = raw.length - 1;
  while (lastLetter >= 0 && /\s/.test(raw[lastLetter].normalize("NFKC")))
    lastLetter--;
  return raw.map((character, index) => {
    let correct: boolean;
    if (/^\s+$/.test(character.normalize("NFKC"))) {
      correct =
        position === 0 || index > lastLetter || expected[position] === " ";
      pendingSpace = position > 0;
    } else {
      if (pendingSpace) {
        position++;
        pendingSpace = false;
      }
      const normalized = Array.from(normalizeCopy(character));
      correct = normalized.every(
        (part, offset) =>
          part.toLowerCase() === expected[position + offset]?.toLowerCase(),
      );
      position += normalized.length;
    }
    return { text: character, state: correct ? "correct" : "incorrect" };
  });
}

export function textSentences(text: string): PracticeSentence[] {
  return text
    .replace(/\r/g, "")
    .split(/\n\s*\n/)
    .flatMap((paragraph) => {
      const clean = paragraph.replace(/\s+/g, " ").trim();
      if (!clean) return [];
      const pieces =
        typeof Intl.Segmenter === "function"
          ? [
              ...new Intl.Segmenter("en", { granularity: "sentence" }).segment(
                clean,
              ),
            ].map((value) => value.segment.trim())
          : (clean.match(/[^.!?]+(?:[.!?]+["'’”]?|$)/g) ?? [clean]);
      const sentences: string[] = [];
      for (const piece of pieces.filter(Boolean)) {
        if (
          sentences.length &&
          /\b(?:Mr|Mrs|Ms|Dr|Prof|Sr|Jr|St|vs)\.$/i.test(sentences.at(-1)!)
        )
          sentences[sentences.length - 1] += " " + piece;
        else sentences.push(piece);
      }
      return sentences.map((value) => ({
        text: value,
        startMs: null,
        endMs: null,
      }));
    });
}

export function pastedDocument(
  title: string,
  text: string,
  kind = "粘贴文本",
): PracticeDocument {
  const sentences = textSentences(text);
  if (!sentences.length) throw new Error("请提供可阅读的英语文本。");
  return {
    id: "",
    title: title.trim() || "我的阅读",
    kind,
    chapters: [{ title: "正文", sentences }],
    audioFile: null,
  };
}

function xml(text: string) {
  const document = new DOMParser().parseFromString(text, "application/xml");
  if (document.querySelector("parsererror"))
    throw new Error("EPUB 目录无法读取，请使用未加密的电子书。");
  return document;
}
const elements = (document: Document, name: string) =>
  [...document.getElementsByTagName("*")].filter(
    (element) => element.localName === name,
  );

export async function readPracticeFile(
  file: File,
): Promise<{ document: PracticeDocument; audio?: Uint8Array }> {
  if (/\.txt$/i.test(file.name))
    return {
      document: pastedDocument(
        file.name.replace(/\.txt$/i, ""),
        await file.text(),
        "TXT 文本",
      ),
    };
  if (!/\.(epub|lesson\.zip)$/i.test(file.name))
    throw new Error("请选择 TXT、未加密的 EPUB 或 .lesson.zip 课程包。");
  const zip = await JSZip.loadAsync(await file.arrayBuffer());
  if (/\.lesson\.zip$/i.test(file.name)) {
    const metadata = zip.file("lesson.json");
    if (!metadata) throw new Error("课程包缺少 lesson.json。");
    const lesson = JSON.parse(await metadata.async("string"));
    if (
      lesson.format !== "english-practice-lesson" ||
      lesson.version !== 1 ||
      typeof lesson.title !== "string" ||
      !Array.isArray(lesson.sentences) ||
      !lesson.sentences.length ||
      !lesson.sentences.every(
        (sentence: { text: string; start: number; end: number }) =>
          typeof sentence.text === "string" &&
          sentence.text.trim() &&
          Number.isFinite(sentence.start) &&
          Number.isFinite(sentence.end) &&
          sentence.start >= 0 &&
          sentence.end > sentence.start,
      )
    )
      throw new Error("课程文字或音频时间位置无效。");
    const audio = typeof lesson.audio === "string" && zip.file(lesson.audio);
    if (!audio) throw new Error("课程包缺少原声音频。");
    return {
      document: {
        id: "",
        title: lesson.title,
        kind: "原声课程",
        audioFile: null,
        chapters: [
          {
            title: "课程",
            sentences: lesson.sentences.map(
              (sentence: { text: string; start: number; end: number }) => ({
                text: sentence.text,
                startMs: Math.round(sentence.start * 1000),
                endMs: Math.round(sentence.end * 1000),
              }),
            ),
          },
        ],
      },
      audio: await audio.async("uint8array"),
    };
  }
  const container = zip.file("META-INF/container.xml");
  if (!container) throw new Error("EPUB 缺少电子书目录。");
  const path = elements(
    xml(await container.async("string")),
    "rootfile",
  )[0]?.getAttribute("full-path");
  const packageFile = path && zip.file(path);
  if (!packageFile) throw new Error("EPUB 缺少书籍索引。");
  const opf = xml(await packageFile.async("string"));
  const items = new Map(
    elements(opf, "item").map((element) => [
      element.getAttribute("id"),
      element,
    ]),
  );
  const chapters: PracticeDocument["chapters"] = [];
  for (const reference of elements(opf, "itemref")) {
    if (reference.getAttribute("linear") === "no") continue;
    const item = items.get(reference.getAttribute("idref"));
    if (!item || item.getAttribute("properties")?.split(/\s+/).includes("nav"))
      continue;
    const url = new URL(
      (item.getAttribute("href") || "").split("#")[0].split("?")[0],
      "https://epub.local/" + path,
    );
    const chapterFile = zip.file(decodeURIComponent(url.pathname.slice(1)));
    if (!chapterFile) throw new Error("EPUB 缺少章节文件。");
    // EPUB chapters are XHTML. Keep their resources inert while extracting text.
    const content = xml(await chapterFile.async("string"));
    const title =
      content.querySelector("h1,h2,h3,title")?.textContent?.trim() ||
      `第 ${chapters.length + 1} 节`;
    content
      .querySelectorAll("script,style,nav,svg")
      .forEach((element) => element.remove());
    content
      .querySelectorAll("br")
      .forEach((element) => element.replaceWith("\n"));
    content
      .querySelectorAll("p,div,h1,h2,h3,h4,h5,h6,li,blockquote,section,tr")
      .forEach((element) => element.append("\n\n"));
    const sentences = textSentences(
      content.getElementsByTagNameNS("*", "body")[0]?.textContent || "",
    );
    if (sentences.length) chapters.push({ title, sentences });
  }
  if (!chapters.length)
    throw new Error("未找到可阅读文字，请使用未加密的 EPUB 或 TXT。");
  return {
    document: {
      id: "",
      title:
        elements(opf, "title")[0]?.textContent?.trim() ||
        file.name.replace(/\.epub$/i, ""),
      kind: "EPUB 电子书",
      chapters,
      audioFile: null,
    },
  };
}

export function adjacentSentence(
  state: PracticeState,
  direction: number,
): PracticeProgress | null {
  let chapterIndex = state.chapterIndex;
  let sentenceIndex = state.sentenceIndex + direction;
  if (sentenceIndex < 0) {
    if (!chapterIndex) return null;
    chapterIndex--;
    sentenceIndex = state.document.chapters[chapterIndex].sentences.length - 1;
  } else if (
    sentenceIndex >= state.document.chapters[chapterIndex].sentences.length
  ) {
    if (chapterIndex === state.document.chapters.length - 1) return null;
    chapterIndex++;
    sentenceIndex = 0;
  }
  return { ...practiceProgress(state), chapterIndex, sentenceIndex, copy: "" };
}
