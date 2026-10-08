import { useEffect, useRef, useState } from "react";
import { useLocalExplanation } from "../localExplanations";
import { call } from "../api";
import type { Candidate, CandidateExample } from "../types";

export function CandidateMeaning({
  candidate,
  sourceId,
  model,
  selected,
  meaning,
}: {
  candidate: Candidate;
  sourceId: string;
  model: string;
  selected: boolean;
  meaning?: string;
}) {
  const element = useRef<HTMLSpanElement>(null);
  const [visible, setVisible] = useState(false);
  const [context, setContext] = useState<{
    candidate: Candidate;
    sourceId: string;
    text: string;
  } | null>(null);
  const active = (visible || selected) && !meaning;
  useEffect(() => {
    if (!active) return;
    let disposed = false;
    call<CandidateExample[]>("candidate_examples", {
      sourceId,
      key: candidate.key,
    })
      .then((values) => {
        if (!disposed)
          setContext({ candidate, sourceId, text: values[0]?.text ?? "" });
      })
      .catch(() => {
        if (!disposed) setContext({ candidate, sourceId, text: "" });
      });
    return () => {
      disposed = true;
    };
  }, [candidate, sourceId, active]);
  useEffect(() => {
    const target = element.current;
    if (!target) return;
    const observer = new IntersectionObserver(
      (values) => setVisible(values.some((value) => value.isIntersecting)),
      {
        rootMargin: "60px",
      },
    );
    observer.observe(target);
    return () => observer.disconnect();
  }, []);
  const result = useLocalExplanation(
    candidate.text,
    context?.candidate === candidate && context.sourceId === sourceId
      ? context.text
      : "",
    model,
    active && context?.candidate === candidate && context.sourceId === sourceId,
  );
  const value = meaning ?? result.value?.meaning;
  return (
    <span
      ref={element}
      className={`definition candidate-meaning${value ? "" : " muted"}`}
      title={value}
    >
      {value ??
        (result.state === "error" ? "暂无中文释义" : "正在准备中文释义…")}
    </span>
  );
}
