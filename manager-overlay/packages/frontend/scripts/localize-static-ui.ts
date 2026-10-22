/** Mechanical source migration. Only authored UI text is touched, never data, enums, CSS, URLs or protocol values. */
import fs from 'node:fs';
import path from 'node:path';
import ts from 'typescript';
const root=path.resolve('src');
const attrs=new Set(['title','placeholder','aria-label','description','label','message','emptyMessage','searchPlaceholder','confirmLabel','cancelLabel','loadingText','helpText','subtitle','alt','emptyText','filteredEmptyText','pageTitle','searchLabel','tableLabel','idleLabel','refreshingLabel','degradedLabel','startingLabel']);
const decode=(text:string)=>text.replace(/&(#x[0-9a-f]+|#\d+|amp|quot|apos|lt|gt|nbsp|mdash|ndash|hellip);/gi,(_,e:string)=>e.startsWith('#')?String.fromCodePoint(parseInt(e.slice(e[1]?.toLowerCase()==='x'?2:1),e[1]?.toLowerCase()==='x'?16:10)):({amp:'&',quot:'"',apos:"'",lt:'<',gt:'>',nbsp:' ',mdash:'—',ndash:'–',hellip:'…'} as Record<string,string>)[e]);
function jsxText(raw:string){const lines=raw.split(/\r?\n/);let last=lines.length-1;while(last>=0&&!lines[last].trim())last--;return lines.map((line,i)=>{line=line.replace(/\t/g,' ');if(i)line=line.replace(/^ +/,'');if(i<lines.length-1)line=line.replace(/ +$/,'');return line+(line&&i<last?' ':'');}).join('');}
function technical(text:string){return !/[a-z]{2}/i.test(text)||/^(?:https?:|ts3image:|\{\{|!|# Netscape|\?safe-ui|\[\{|\{"|\[cspacer|[A-Za-z]+\/[A-Za-z_]+$)/.test(text.trim());}
let files=0,editsTotal=0;
function migrate(file:string){
 let source=fs.readFileSync(file,'utf8'); const alreadyLocalized=source.includes('translate as uiText');
 const sf=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX);
 const edits:Array<{start:number;end:number;text:string}>=[];
 const replace=(node:ts.Node,text:string)=>edits.push({start:node.getStart(sf),end:node.end,text});
 const format=(node:ts.Expression):string|null=>{
  if(ts.isStringLiteral(node)||ts.isNoSubstitutionTemplateLiteral(node)){const text=decode(node.text);return technical(text)?null:`uiText(${JSON.stringify(text)})`;}
  if(ts.isTemplateExpression(node)){
   const displayValue=(value:ts.Expression):string=>{
    if(ts.isParenthesizedExpression(value))return `(${displayValue(value.expression)})`;
    if(ts.isConditionalExpression(value))return `(${value.condition.getText(sf)} ? ${displayValue(value.whenTrue)} : ${displayValue(value.whenFalse)})`;
    if(ts.isStringLiteral(value)&&!technical(value.text))return `uiText(${JSON.stringify(value.text)})`;
    return value.getText(sf);
   };
   const values=node.templateSpans.map(s=>displayValue(s.expression));
   const text=node.head.text+node.templateSpans.map((s,i)=>`{${i}}${s.literal.text}`).join('');
   return technical(text)?null:`uiText(${JSON.stringify(text)}, ${values.join(', ')})`;
  }
  return null;
 };
 const branch=(node:ts.Expression)=>{
  if(ts.isParenthesizedExpression(node)){branch(node.expression);return;}
  const value=format(node);if(value){replace(node,value);return;}
  if(ts.isConditionalExpression(node)){branch(node.whenTrue);branch(node.whenFalse);}
  if(ts.isTemplateExpression(node))for(const span of node.templateSpans)branch(span.expression);
  // Only the rendered side of && / ||, never comparisons or condition values.
  if(ts.isBinaryExpression(node)&&[ts.SyntaxKind.AmpersandAmpersandToken,ts.SyntaxKind.BarBarToken,ts.SyntaxKind.QuestionQuestionToken].includes(node.operatorToken.kind))branch(node.right);
 };
 const isTechnicalNode=(node:ts.Node)=>{for(let p=node.parent;p;p=p.parent)if(ts.isJsxElement(p)&&['code','pre'].includes(p.openingElement.tagName.getText(sf)))return true;return false;};
 const visit=(node:ts.Node)=>{
  if(ts.isJsxText(node)&&!isTechnicalNode(node)){const text=decode(jsxText(node.text));if(!technical(text))replace(node,`{uiText(${JSON.stringify(text)})}`);}
  if(ts.isJsxAttribute(node)&&attrs.has(node.name.getText(sf))&&node.initializer){if(ts.isStringLiteral(node.initializer)){const text=decode(node.initializer.text);if(!technical(text))replace(node.initializer,`{uiText(${JSON.stringify(text)})}`);}else if(ts.isJsxExpression(node.initializer)&&node.initializer.expression)branch(node.initializer.expression);}
  if(ts.isJsxExpression(node)&&node.expression&&!ts.isJsxAttribute(node.parent)&&!isTechnicalNode(node))branch(node.expression);
  if(ts.isCallExpression(node)&&ts.isPropertyAccessExpression(node.expression)&&node.expression.expression.getText(sf)==='toast'&&node.arguments[0])branch(node.arguments[0]);
  if(ts.isCallExpression(node)&&ts.isIdentifier(node.expression)&&['setError','setDraftError','setFormError','setBatchProgress','setAddBatchProgress','setNotice'].includes(node.expression.text)&&node.arguments[0])branch(node.arguments[0]);
  if(ts.isCallExpression(node)&&ts.isIdentifier(node.expression)&&['uiText','tr','t','translate'].includes(node.expression.text))for(const arg of node.arguments.slice(1))if(ts.isConditionalExpression(arg)||ts.isParenthesizedExpression(arg))branch(arg);
  ts.forEachChild(node,visit);
 };visit(sf);
 // De-duplicate expressions visited both by an attribute and JSX expression.
 const unique=[...new Map(edits.map(e=>[`${e.start}:${e.end}`,e])).values()];
 if(!unique.length)return;
 const hooks:Array<{start:number;end:number;text:string}>=[];
 const inspect=(node:ts.Node)=>{
  if(ts.isFunctionDeclaration(node)&&node.name&&/^[A-Z]/.test(node.name.text)&&node.body&&node.getText(sf).includes('<'))hooks.push({start:node.body.getStart(sf)+1,end:node.body.getStart(sf)+1,text:'\n  useUiLanguageStore(state => state.language);'});
  if(ts.isVariableDeclaration(node)&&ts.isIdentifier(node.name)&&/^[A-Z]/.test(node.name.text)&&node.initializer){
   let fn:ts.Node=node.initializer;
   if(ts.isCallExpression(fn)&&/^(React\.)?(memo|forwardRef)$/.test(fn.expression.getText(sf)))fn=fn.arguments[0];
   if(fn&&(ts.isArrowFunction(fn)||ts.isFunctionExpression(fn))&&fn.getText(sf).includes('<')){
    const b=fn.body;if(ts.isBlock(b))hooks.push({start:b.getStart(sf)+1,end:b.getStart(sf)+1,text:'\n  useUiLanguageStore(state => state.language);'});
    else{hooks.push({start:b.getStart(sf),end:b.getStart(sf),text:'{ useUiLanguageStore(state => state.language); return '});hooks.push({start:b.end,end:b.end,text:'; }'});}
   }
  }
  ts.forEachChild(node,inspect);
 };if(!alreadyLocalized)inspect(sf);
 const all=[...unique,...hooks].sort((a,b)=>b.start-a.start||b.end-a.end);
 for(const e of all)source=source.slice(0,e.start)+e.text+source.slice(e.end);
 if(!alreadyLocalized)source="import { translate as uiText, useLanguageStore as useUiLanguageStore } from '@/lib/i18n';\n"+source;
 fs.writeFileSync(file,source);files++;editsTotal+=unique.length;
}
function walk(dir:string){for(const e of fs.readdirSync(dir,{withFileTypes:true})){const file=path.join(dir,e.name);if(e.isDirectory())walk(file);else if(file.endsWith('.tsx'))migrate(file);}}
walk(root);console.log(JSON.stringify({files,staticUiExpressions:editsTotal}));
