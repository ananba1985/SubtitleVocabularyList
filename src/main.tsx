import React from 'react';
import ReactDOM from 'react-dom/client';
import { invoke } from '@tauri-apps/api/core';
import './styles.css';

function App() {
  const [status, setStatus] = React.useState('正在连接本地词库…');
  React.useEffect(() => {
    invoke<{ dataDirectory: string; schemaVersion: number }>('app_info')
      .then(info => setStatus(`本地词库已准备 · 数据版本 ${info.schemaVersion}`))
      .catch(error => setStatus(String(error?.message ?? error)));
  }, []);
  return <main className="app-shell">
    <aside><div className="brand">SV<span>SubtitleVocabularyList</span></div>
      <p>从对白开始，记住每一次相遇。</p></aside>
    <section><p className="eyebrow">本地英语单词本 · 0.1</p>
      <h1>让听过的单词，成为自己的表达。</h1>
      <p className="status">{status}</p>
      <p>桌面基础工程正在开发，剧集导入、收录和复习将在后续增量接入。</p>
    </section>
  </main>;
}

ReactDOM.createRoot(document.getElementById('root')!).render(<React.StrictMode><App /></React.StrictMode>);
