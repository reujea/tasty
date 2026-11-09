//! 시작 화면(연결 · 에이전트 관리 · 역할 관리)의 CSS.
//!
//! 색 · 글꼴 · 간격은 맨 위 사용자 정의 속성에 모은다 — 나중에 Theme 값을 그 자리에 넣으면 된다.
//! 라벨 · 단추 · 제목은 시스템 글꼴이고, id · 경로 · 명령 · 환경처럼 코드인 값만 고정폭(`.mono`)이다.
//! 간격은 4px 격자(4 · 8 · 12 · 16 · 28), 선은 1px 이다.
//!
//! 화면에 나가는 CSS 에는 주석을 안 단다 — 문서에 그대로 실린다.

pub(super) const CSS: &str = r#"<style>
 :root{color-scheme:dark light;
  --bg:#16181d;--fg:#c8ccd4;--fg-strong:#e6e9ef;--muted:#8b93a1;--faint:#6b7280;
  --border:#2a2e37;--sep:#21242b;--input-bg:#0f1114;--head-bg:#1b1e24;--hover:#1c1f26;
  --row-on:#2b3444;--btn-bg:#1f232b;--btn-hover:#262b34;--btn-border-hover:#3a4150;
  --accent:#5ac17d;--accent-fg:#0f1114;--warn:#d8a33c;--danger:#e06c75;--danger-border:#4a2f33;
  --err-bg:#2a1f21;--err-border:#5a3438;--seg-bg:#0f1114;--seg-on:#2a2e37;
  --font-ui:-apple-system,BlinkMacSystemFont,"Segoe UI","Apple SD Gothic Neo","Noto Sans KR",Helvetica,Arial,sans-serif;
  --font-mono:ui-monospace,"SF Mono",Menlo,Consolas,monospace;
  --fs:13px;--fs-sm:12px;--s1:4px;--s2:8px;--s3:12px;--s4:16px;--s7:28px;--ctl-h:28px;--radius:4px}
 body{font:var(--fs)/1.5 var(--font-ui);margin:0;padding:var(--s7) var(--s4);background:var(--bg);color:var(--fg)}
 .mono,code{font-family:var(--font-mono);font-size:var(--fs-sm)}
 h1{font-size:14px;font-weight:600;margin:0 0 var(--s1);color:var(--fg-strong)}
 h2{font-size:var(--fs);font-weight:600;margin:var(--s7) 0 var(--s2);color:var(--fg-strong)}
 p.sub{color:var(--muted);margin:0 0 var(--s4)}
 input,select,textarea{box-sizing:border-box;width:100%;background:var(--input-bg);border:1px solid var(--border);
  color:var(--fg);border-radius:var(--radius);font:inherit}
 input,select{height:var(--ctl-h);padding:0 var(--s2)}
 textarea{padding:var(--s1) var(--s2);min-height:72px;resize:vertical;line-height:1.5}
 input,textarea{cursor:text}
 input[type=checkbox]{width:auto;height:auto;margin:0;cursor:pointer}
 input[readonly]{color:var(--muted);background:var(--bg)}
 input:focus,select:focus,textarea:focus{outline:none;border-color:var(--accent)}
 ::placeholder{color:var(--faint);opacity:1}
 optgroup{color:var(--muted)} option{color:var(--fg);background:var(--input-bg)}
 .help{margin:var(--s1) 0 0;color:var(--muted);font-size:var(--fs-sm)}
 .note{color:var(--muted);font-size:var(--fs-sm)}
 .btn{display:inline-flex;align-items:center;justify-content:center;gap:var(--s1);box-sizing:border-box;
      height:24px;padding:0 var(--s2);border:1px solid var(--border);border-radius:var(--radius);
      background:var(--btn-bg);color:var(--fg);font:inherit;font-size:var(--fs-sm);line-height:1;
      text-decoration:none;white-space:nowrap;cursor:pointer}
 .btn:hover{background:var(--btn-hover);border-color:var(--btn-border-hover);color:var(--fg-strong)}
 .btn:active{background:var(--input-bg)}
 .btn.danger{color:var(--danger);border-color:var(--danger-border)}
 .btn.off{opacity:.4;cursor:default}
 .btn.tall{height:var(--ctl-h)}
 button.primary{height:var(--ctl-h);padding:0 var(--s4);background:var(--accent);border:0;border-radius:var(--radius);
  color:var(--accent-fg);font:inherit;font-weight:600;cursor:pointer}
 .linkbtn{background:none;border:0;padding:0;color:var(--accent);font:inherit;font-size:var(--fs-sm);cursor:pointer}
 .linkbtn:hover{text-decoration:underline}
 .seg{display:inline-flex;gap:2px;padding:2px;margin:0 0 var(--s4);background:var(--seg-bg);
  border:1px solid var(--border);border-radius:6px}
 .seg .tab{height:24px;padding:0 var(--s3);background:none;border:0;border-radius:var(--radius);
  color:var(--muted);font:inherit;cursor:pointer}
 .seg .tab:hover{color:var(--fg)}
 .seg .tab.on{background:var(--seg-on);color:var(--fg-strong);font-weight:600}
 .pane{display:none} .pane.on{display:block}
 [hidden]{display:none !important}
 .fgrid{display:grid;grid-template-columns:120px minmax(0,1fr);gap:var(--s2) var(--s3);align-items:start;max-width:640px}
 .fgrid>label{margin:0;text-align:right;color:var(--muted);line-height:var(--ctl-h)}
 .fgrid>.ctl{min-width:0}
 .fgrid>.full{grid-column:1/-1}
 .fgrid>.factions{grid-column:2;display:flex;justify-content:flex-end;gap:var(--s2);margin-top:var(--s1)}
 .with-btn{display:flex;gap:var(--s2);align-items:center}
 .with-btn input{flex:1;min-width:0}
 .with-btn .btn{height:var(--ctl-h)}
 .ctl select+input{margin-top:var(--s1)}
 .connect-split{display:grid;grid-template-columns:minmax(0,640px) minmax(280px,1fr);gap:var(--s7);align-items:start}
 .connect-main{min-width:0}
 .rolebox{min-width:0}
 .rolebox>h2{margin-top:0}
 .rolebox .tools{margin-top:0}
 .rolebox .noguide{color:var(--muted);font-size:var(--fs-sm)}
 #rpick tbody tr{cursor:pointer}
 .pager{display:flex;gap:var(--s2);align-items:center;justify-content:flex-end;margin-top:var(--s2)}
 .roles-empty{margin:0;color:var(--muted)}
 details.more{grid-column:1/-1;border-top:1px solid var(--border);padding-top:var(--s2)}
 details.more>summary{color:var(--muted);cursor:pointer;user-select:none;list-style:none;padding:var(--s1) 0;margin-left:132px}
 details.more>summary::-webkit-details-marker{display:none}
 details.more>summary::before{content:"\25B8  ";color:var(--accent)}
 details.more[open]>summary::before{content:"\25BE  "}
 details.more>summary:hover{color:var(--fg)}
 details.more>.fgrid{margin-top:var(--s2)}
 .known{margin-top:var(--s3);color:var(--muted)}
 .known a{color:var(--accent);text-decoration:none;margin-right:var(--s3)}
 .hint{color:var(--muted);margin-top:var(--s4);font-size:var(--fs-sm)}
 .sect{margin-top:var(--s7);padding-top:var(--s4);border-top:1px solid var(--border)}
 .sect>h2:first-child{margin-top:0}
 .none{color:var(--muted);margin:var(--s2) 0}
 .tools{display:flex;flex-wrap:wrap;gap:var(--s2);align-items:center;margin-top:var(--s2)}
 .tools input{flex:1;min-width:140px}
 .tools .btn{height:var(--ctl-h)}
 .count{color:var(--muted);font-size:var(--fs-sm);white-space:nowrap;font-variant-numeric:tabular-nums}
 .bulk{display:flex;flex-wrap:wrap;gap:var(--s2);align-items:center;margin-top:var(--s2);font-size:var(--fs-sm);color:var(--muted)}
 .tbl-wrap{overflow:auto;max-height:min(60vh,520px);margin-top:var(--s2);border:1px solid var(--border);border-radius:6px}
 .tbl{width:100%;border-collapse:separate;border-spacing:0;table-layout:fixed}
 .tbl th,.tbl td{height:var(--ctl-h);padding:0 var(--s2);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;
  text-align:left;vertical-align:middle;border-bottom:1px solid var(--sep)}
 .tbl th{position:sticky;top:0;z-index:1;background:var(--head-bg);color:var(--muted);font-size:var(--fs-sm);font-weight:600}
 .tbl tbody tr:last-child td{border-bottom:0}
 .tbl tbody tr:hover{background:var(--hover)}
 .tbl .num{text-align:right;font-variant-numeric:tabular-nums}
 .tbl .nil{color:var(--faint)}
 .tbl .mid{display:block;overflow:hidden;white-space:nowrap;text-overflow:ellipsis}
 .tbl td.mono{font-size:var(--fs-sm)}
 .tbl .pair{display:flex;gap:var(--s1);min-width:0}
 .tbl .pair .mid{flex:0 1 auto;min-width:0}
 .tbl .pair .aname{flex:none;max-width:50%;overflow:hidden;text-overflow:ellipsis;font-family:var(--font-ui)}
 .tbl td.acts{text-align:right;overflow:visible}
 .tbl td.acts>*{opacity:0}
 .tbl tbody tr:hover td.acts>*,.tbl tbody tr:focus-within td.acts>*,.tbl td.acts.asking>*{opacity:1}
 @media (hover:none){.tbl td.acts>*{opacity:1}}
 .tbl td.acts .btn+.btn{margin-left:var(--s1)}
 .tbl th.pickcol,.tbl td.pickcol{padding:0;text-align:center}
 .dot{display:inline-block;width:8px;height:8px;border-radius:50%;margin-right:var(--s2);vertical-align:middle}
 .run{background:var(--accent)} .slept{background:var(--warn)} .dead{background:var(--faint)}
 .inherit{color:var(--muted)}
 .inuse{color:var(--fg)}
 .aname{color:var(--muted)}
 .rem{color:var(--warn)}
 .guide-no{color:var(--faint)}
 #rows tbody tr{cursor:pointer}
 #rows tr.on{background:var(--row-on)}
 #rows tr.on td{color:var(--fg-strong)}
 .confirm{color:var(--warn);font-size:var(--fs-sm)}
 .confirm a{margin-left:var(--s2);text-decoration:none}
 .confirm .yes{color:var(--danger)}
 .confirm .no{color:var(--muted)}
 .mg-note{margin:0 0 var(--s4);padding:var(--s2) var(--s3);border-radius:var(--radius);border:1px solid var(--border);max-width:640px;box-sizing:border-box}
 .mg-note.ok{color:var(--accent)}
 .mg-note.err{color:var(--danger);background:var(--err-bg);border-color:var(--err-border)}
 #starting{position:fixed;inset:0;background:rgba(22,24,29,.92);display:none;
   flex-direction:column;align-items:center;justify-content:center;gap:var(--s4);z-index:9}
 #starting.on{display:flex}
 .spin{width:24px;height:24px;border:3px solid var(--border);border-top-color:var(--accent);
   border-radius:50%;animation:sp .8s linear infinite}
 @keyframes sp{to{transform:rotate(360deg)}}
 .starting-msg{color:var(--muted)}
 .start-err{background:var(--err-bg);border:1px solid var(--err-border);color:var(--danger);
   border-radius:var(--radius);padding:var(--s2) var(--s3);margin:var(--s3) 0;max-width:640px;box-sizing:border-box}
 @media (max-width:900px){.connect-split{grid-template-columns:minmax(0,1fr)}}
 @media (max-width:520px){
  body{padding:var(--s4) var(--s3)}
  .fgrid{grid-template-columns:minmax(0,1fr)}
  .fgrid>label{text-align:left;line-height:1.5}
  .fgrid>.factions{grid-column:1}
  details.more>summary{margin-left:0}
 }
</style>
"#;
