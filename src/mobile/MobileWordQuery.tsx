import { useEffect, useState } from "react";
import { call, message, uid, waitTask } from "../api";
import type { Explanation } from "../types";
import { MobileQuery } from "./MobileQuery";

export function MobileWordQuery({ target, context, close, collect }: {
  target:string; context:string; close:()=>void; collect:(text:string,kind:string,meaning:string)=>void;
}) {
  const [text,setText] = useState(target), [kind,setKind] = useState(target.includes(" ") ? "phrase" : "word");
  const [result,setResult] = useState<Explanation|null>(null), [error,setError] = useState("");
  const [busy,setBusy] = useState(false), [request,setRequest] = useState(target);
  const [attempt,setAttempt] = useState(0);
  useEffect(() => {
    let disposed = false; setBusy(true); setError(""); setResult(null);
    call("explain_start",{text:request,context,operationId:uid()}).then((started) => waitTask(started)).then((value) => { if (!disposed) setResult(value); })
      .catch((e) => { if (!disposed) setError(message(e)); }).finally(() => { if (!disposed) setBusy(false); });
    return () => { disposed = true; };
  }, [request,context,attempt]);
  return <section className="mobile-query mobile-card">
    <div className="toolbar"><strong>离线查词</strong><button onClick={close}>关闭查词</button></div>
    <p className="muted">词典释义供参考，收录前可结合原句修改。</p>
    <label>单词或短语<input aria-label="查询单词或短语" value={text} onChange={(e) => setText(e.target.value)} /></label>
    <button disabled={!text.trim()||busy} onClick={() => {setRequest(text.trim());setAttempt((value) => value+1);}}>查询</button>
    <label>收录类型<select value={kind} onChange={(e) => setKind(e.target.value)}><option value="word">单词</option><option value="phrase">短语</option></select></label>
    {busy && <p role="status">正在查询本机资料…</p>}
    {error && <p className="notice">{error}</p>}
    {result && text.trim()===request && <><p>释义：{result.meaning}</p>{result.translation && <p>原句译文：{result.translation}</p>}</>}
    <MobileQuery text={text} context={context} apply={value=>collect(value.text,value.kind,value.meaning)} />
    <button disabled={!text.trim()||busy} onClick={() => collect(text.trim(),kind,text.trim() === request ? result?.meaning ?? "" : "")}>确认收录所查内容</button>
  </section>;
}
