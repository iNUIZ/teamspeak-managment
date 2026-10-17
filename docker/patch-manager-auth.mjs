import fs from 'node:fs';

const path = '/app/packages/backend/dist/routes/auth.routes.js';
let source = fs.readFileSync(path, 'utf8');

const expiryCheck = 'if (stored.expiresAt < new Date() || !stored.user.enabled) {';
if (!source.includes(expiryCheck)) {
  throw new Error('Manager auth layout changed: expiry check not found');
}
source = source.replace(expiryCheck, `if (stored.replacedBy) {
            console.warn(\`[SECURITY] Refresh token reuse detected for family \${stored.family}. Revoking the session.\`);
            await prisma.refreshToken.deleteMany({
                where: stored.family ? { family: stored.family } : { userId: stored.userId },
            });
            throw new AppError(401, 'Invalid refresh token');
        }
        ${expiryCheck}`);

const markPattern = /where:\s*\{\s*id:\s*stored\.id\s*\},\s*data:\s*\{\s*replacedBy:\s*newRefreshToken\s*\}/;
if (!markPattern.test(source)) {
  throw new Error('Manager auth layout changed: rotation update not found');
}
source = source.replace(markPattern, 'where: { id: stored.id, replacedBy: null },\n            data: { replacedBy: newRefreshToken }');

const raceBlock = `if (marked.count === 0) {
            throw new AppError(401, 'Invalid refresh token');
        }`;
if (!source.includes(raceBlock)) {
  throw new Error('Manager auth layout changed: concurrent rotation guard not found');
}
source = source.replace(raceBlock, `if (marked.count === 0) {
            await prisma.refreshToken.deleteMany({
                where: stored.family ? { family: stored.family } : { userId: stored.userId },
            });
            throw new AppError(401, 'Invalid refresh token');
        }`);

const deleteOld = /\n\s*\/\/ Delete old token after creating new one[^\n]*\n\s*await prisma\.refreshToken\.deleteMany\(\{ where: \{ id: stored\.id \} \}\);/;
if (!deleteOld.test(source)) {
  throw new Error('Manager auth layout changed: old-token deletion not found');
}
source = source.replace(deleteOld, '\n        // Keep the consumed token as a reuse-detection tombstone until expiry.');

fs.writeFileSync(path, source);
