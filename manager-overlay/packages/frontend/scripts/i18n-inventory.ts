import fs from 'node:fs';
import path from 'node:path';
import ts from 'typescript';
import { translate, useLanguageStore, hasUiTranslation } from '../src/lib/i18n.js';
import { technicalUiText } from './i18n-technical.js';

useLanguageStore.setState({language:'tr'});
const root=path.resolve('src');
const attrs=new Set(['title','placeholder','aria-label','description','label','message','emptyMessage','searchPlaceholder','confirmLabel','cancelLabel','loadingText','helpText','subtitle','alt','emptyText','filteredEmptyText','pageTitle','searchLabel','tableLabel','idleLabel','refreshingLabel','degradedLabel','startingLabel']);
const hits: Array<{file:string;line:number;kind:string;text:string;translated:boolean}>=[];
const decode=(text:string)=>text.replace(/&(#x[0-9a-f]+|#\d+|amp|quot|apos|lt|gt|nbsp|mdash|ndash|hellip);/gi,(_,entity:string)=>{
 const named:Record<string,string>={amp:'&',quot:'"',apos:"'",lt:'<',gt:'>',nbsp:' ',mdash:'—',ndash:'–',hellip:'…'};
 return entity.startsWith('#')?String.fromCodePoint(parseInt(entity.slice(entity[1]?.toLowerCase()==='x'?2:1),entity[1]?.toLowerCase()==='x'?16:10)):named[entity];
});
const add=(sf:ts.SourceFile,node:ts.Node,text:string,kind:string)=>{
 text=decode(text).replace(/\s+/g,' ').trim();
 if(!/[a-zA-Z]{2}/.test(text))return;
 hits.push({file:path.relative(root,sf.fileName).replaceAll('\\','/'),line:sf.getLineAndCharacterOfPosition(node.getStart(sf)).line+1,kind,text,translated:hasUiTranslation(text)});
};
function scan(file:string){
 const sf=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX);
 const rendered=(node:ts.Expression,kind:string)=>{
  if(ts.isParenthesizedExpression(node))rendered(node.expression,kind);
  else if(ts.isStringLiteral(node)||ts.isNoSubstitutionTemplateLiteral(node))add(sf,node,node.text,kind);
  else if(ts.isConditionalExpression(node)){rendered(node.whenTrue,kind);rendered(node.whenFalse,kind);}
  else if(ts.isTemplateExpression(node)){
   add(sf,node,node.head.text+node.templateSpans.map((span,i)=>`{${i}}${span.literal.text}`).join(''),kind);
   for(const span of node.templateSpans)if(ts.isConditionalExpression(span.expression))rendered(span.expression,kind);
  } else if(ts.isBinaryExpression(node)&&[ts.SyntaxKind.AmpersandAmpersandToken,ts.SyntaxKind.BarBarToken,ts.SyntaxKind.QuestionQuestionToken].includes(node.operatorToken.kind))rendered(node.right,kind);
 };
 const visit=(node:ts.Node)=>{
  if(ts.isJsxText(node)&&!['code','pre'].includes((node.parent as any)?.openingElement?.tagName?.getText(sf))) add(sf,node,node.text,'jsx');
  if(ts.isJsxAttribute(node)&&attrs.has(node.name.getText(sf))&&node.initializer&&ts.isStringLiteral(node.initializer))add(sf,node,node.initializer.text,'attribute');
  if(ts.isCallExpression(node)&&ts.isIdentifier(node.expression)&&['tr','t','translate','uiText'].includes(node.expression.text)&&node.arguments[0])rendered(node.arguments[0],ts.isTemplateExpression(node.arguments[0])?'runtime-template':'translated-call');
  if(ts.isJsxExpression(node)&&node.expression&&(!ts.isJsxAttribute(node.parent)||attrs.has(node.parent.name.getText(sf)))){
   let technical=false;for(let p:ts.Node|undefined=node.parent;p;p=p.parent)if(ts.isJsxElement(p)&&['code','pre'].includes(p.openingElement.tagName.getText(sf)))technical=true;
   if(!technical)rendered(node.expression,'jsx-expression');
  }
  if(ts.isCallExpression(node)&&ts.isIdentifier(node.expression)&&['apiErrorMessage','apiErrorPresentation','translateUiError'].includes(node.expression.text)&&node.arguments[1]&&ts.isStringLiteral(node.arguments[1]))add(sf,node,node.arguments[1].text,'error-fallback');
  if(ts.isCallExpression(node)&&ts.isPropertyAccessExpression(node.expression)&&node.expression.expression.getText(sf)==='toast'&&node.arguments[0]&&ts.isStringLiteral(node.arguments[0]))add(sf,node,node.arguments[0].text,'toast');
  if(ts.isCallExpression(node)&&ts.isIdentifier(node.expression)&&['setError','setDraftError','setFormError','setBatchProgress','setAddBatchProgress','setNotice'].includes(node.expression.text)&&node.arguments[0]&&ts.isStringLiteral(node.arguments[0]))add(sf,node,node.arguments[0].text,'setter');
  if(ts.isPropertyAssignment(node)&&['label','description','title','header','placeholder','hostHint','notes','note','feature','help','hint','action','message'].includes(node.name.getText(sf).replace(/['"]/g,''))&&ts.isStringLiteral(node.initializer))add(sf,node,node.initializer.text,'metadata');
  if(ts.isPropertyAssignment(node)&&ts.isStringLiteral(node.initializer)&&ts.isObjectLiteralExpression(node.parent)&&ts.isVariableDeclaration(node.parent.parent)&&node.parent.parent.name.getText(sf)==='FIELD_HELP')add(sf,node,node.initializer.text,'metadata');
  if(sf.fileName.replaceAll('\\','/').endsWith('/data/bot-templates.ts')&&ts.isPropertyAssignment(node)&&node.name.getText(sf)==='name'&&ts.isStringLiteral(node.initializer))add(sf,node,node.initializer.text,'metadata');
  if(ts.isPropertyAssignment(node)&&ts.isStringLiteral(node.initializer)&&ts.isObjectLiteralExpression(node.parent)&&ts.isVariableDeclaration(node.parent.parent)&&/(?:_LABELS?|_CATEGORIES|descriptions)$/.test(node.parent.parent.name.getText(sf)))add(sf,node,node.initializer.text,'metadata');
  if(ts.isVariableDeclaration(node)&&node.name.getText(sf)==='STEPS'&&node.initializer&&ts.isAsExpression(node.initializer)&&ts.isArrayLiteralExpression(node.initializer.expression))for(const item of node.initializer.expression.elements)if(ts.isStringLiteral(item))add(sf,item,item.text,'metadata');
  ts.forEachChild(node,visit);
 };visit(sf);
}
function walk(dir:string){for(const entry of fs.readdirSync(dir,{withFileTypes:true})){const file=path.join(dir,entry.name);if(entry.isDirectory())walk(file);else if(/\.tsx?$/.test(file)&&!entry.name.startsWith('tr-'))scan(file);}}
walk(root);
const reportIndex=process.argv.indexOf('--report');
if(reportIndex>=0&&process.argv[reportIndex+1])fs.writeFileSync(path.resolve(process.argv[reportIndex+1]),JSON.stringify(hits,null,2));
const missing=[...new Set(hits.filter(h=>!h.translated).map(h=>h.text))].sort();
if(!process.argv.includes('--check')) {
 console.log(JSON.stringify({hits:hits.length,withoutCatalogEntry:missing.length}));
 console.log(missing.join('\n'));
}
if(process.argv.includes('--check')) {
 const unexpected=hits.filter(h=>!h.translated&&!technicalUiText.has(h.text)&&!/[çğıöşüÇĞİÖŞÜ]/.test(h.text));
 const unwrapped=hits.filter(h=>['jsx','jsx-expression','attribute','toast','runtime-template'].includes(h.kind)&&!technicalUiText.has(h.text)&&!/[çğıöşüÇĞİÖŞÜ]/.test(h.text));
 console.log(JSON.stringify({examined:hits.length,missingInterfaceTranslations:new Set(unexpected.map(h=>h.text)).size,unwrappedInterfaceText:unwrapped.length}));
 if(unexpected.length||unwrapped.length) { console.error('Missing Turkish UI catalog entries:',JSON.stringify(unexpected,null,2));console.error('Unwrapped UI text:',JSON.stringify(unwrapped,null,2));process.exitCode=1; }
 else console.log('Turkish catalog coverage passed; protocol examples and authored Turkish source excluded.');
}
