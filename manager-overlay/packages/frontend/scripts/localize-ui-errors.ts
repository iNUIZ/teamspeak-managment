/** Narrow AST migration of raw service toast errors; fallback text stays actionable. */
import fs from 'node:fs';
import path from 'node:path';
import ts from 'typescript';
function visitFile(file:string) {
 let source=fs.readFileSync(file,'utf8');
 const sf=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX);
 const edits:Array<{start:number;end:number;text:string}>=[];
 let needsError=false;
 function walk(node:ts.Node) {
  if(ts.isCallExpression(node)&&['toast.error','setError','setDraftError','setFormError'].includes(node.expression.getText(sf))&&node.arguments[0]) {
   const arg=node.arguments[0]; const text=arg.getText(sf);
   if(!text.includes('uiError(')&& /response\?\.data|\.errors\??\.?\[/.test(text)&&ts.isBinaryExpression(arg)&&arg.operatorToken.kind===ts.SyntaxKind.BarBarToken) {
    edits.push({start:arg.getStart(sf),end:arg.end,text:`uiError(String(${text}), ${arg.right.getText(sf)})`});needsError=true;
   }
  }
  if(ts.isCallExpression(node)&&node.expression.getText(sf)==='useMemo'&&node.arguments[1]&&ts.isArrayLiteralExpression(node.arguments[1])&&source.includes('useUiLanguageStore')&&/\b(uiText|tr|t|translate)\(/.test(node.arguments[0].getText(sf))&&!node.arguments[1].getText(sf).includes('useUiLanguageStore.getState')) {
   const deps=node.arguments[1];
   edits.push({start:deps.end-1,end:deps.end-1,text:(deps.elements.length?', ':'')+'useUiLanguageStore.getState().language'});
  }
  ts.forEachChild(node,walk);
 }
 walk(sf);
 if(!edits.length)return;
 for(const e of edits.sort((a,b)=>b.start-a.start))source=source.slice(0,e.start)+e.text+source.slice(e.end);
 if(needsError&&!source.includes('translateUiError as uiError'))source="import { translateUiError as uiError } from '@/lib/i18n';\n"+source;
 fs.writeFileSync(file,source);
}
function walk(dir:string){for(const item of fs.readdirSync(dir,{withFileTypes:true})){const file=path.join(dir,item.name);if(item.isDirectory())walk(file);else if(/\.tsx?$/.test(file)&&!item.name.startsWith('tr-'))visitFile(file);}}
walk('src');
