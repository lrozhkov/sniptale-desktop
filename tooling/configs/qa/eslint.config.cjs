'use strict';
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const modules = path.join(root, '.tmp/qa-tools/script-tools/npm/node_modules');
const load = require('node:module').createRequire(path.join(modules, '../package.json'));
const tseslint = load('typescript-eslint');
const promise = load('eslint-plugin-promise');
const security = load('eslint-plugin-security');
const globals = load('globals');
module.exports = [{
  files: ['**/*.js', '**/*.cjs'],
  languageOptions: {
    parser: tseslint.parser,
    parserOptions: {project: path.join(__dirname, 'script-node/tsconfig.json'), tsconfigRootDir: root},
    globals: globals.node,
    sourceType: 'commonjs',
  },
  plugins: {'@typescript-eslint': tseslint.plugin, promise, security},
  linterOptions: {reportUnusedDisableDirectives: 'error'},
  rules: {
    'constructor-super': 'error', 'for-direction': 'error', 'getter-return': 'error',
    'no-async-promise-executor': 'error', 'no-constant-binary-expression': 'error',
    'no-dupe-args': 'error', 'no-dupe-keys': 'error', 'no-duplicate-case': 'error',
    'no-eval': 'error', 'no-implied-eval': 'error', 'no-new-func': 'error',
    'no-promise-executor-return': 'error', 'no-unreachable': 'error',
    'no-unsafe-finally': 'error', 'no-unsafe-optional-chaining': 'error',
    'no-unused-vars': ['error', {argsIgnorePattern: '^_'}], 'valid-typeof': 'error',
    '@typescript-eslint/no-floating-promises': ['error', {ignoreVoid: false}],
    '@typescript-eslint/no-misused-promises': 'error',
    'promise/catch-or-return': 'error', 'promise/no-return-wrap': 'error',
    'security/detect-eval-with-expression': 'error',
    'security/detect-non-literal-require': 'error',
    'security/detect-unsafe-regex': 'error',
  },
}];
