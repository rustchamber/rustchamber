# Conversas/Sessões e Projetos na sidebar

Estudo técnico do OpenChamber para reconstrução no RustChamber (egui/Rust).

Data: 2026-11-06
Fontes lidas:

- `OC/` = `labs/openchamber/packages/ui/src/`
- `OCS/` = `labs/openchamber/packages/web/server/`
- `OP/` = `labs/opencode/`
- `RC/` = `crates/`

O OpenChamber deste checkout não corresponde exatamente aos screenshots: o modal de projeto tem outra apresentação, o menu de display está em outra ordem e “Marcar tudo como lido” não existe no código. As diferenças estão marcadas ao longo do texto, principalmente em 2.10 e 2.14. O comportamento descrito abaixo é o do código; quando o screenshot diverge, o texto diz qual é a diferença e o que deve ser reproduzido.

---

## 1. Resumo executivo e princípios de design

### 1.1 O modelo em uma frase

A sidebar do OpenChamber não é um banco de dados de sessões: é uma **projeção ordenada** sobre três autoridades distintas.

```
┌─────────────────────────────────────────────────────────────────────┐
│ OpenCode server                                                     │
│ • Session records (title, parentID, location, time.idle/viewed,     │
│   archived, metadata)                                               │
│ • Projects (GET /api/project)                                       │
│ • Messages / events                                                 │
└─────────────────────────────────────────────────────────────────────┘
                 ▲
                 │ HTTP + SSE
┌─────────────────────────────────────────────────────────────────────┐
│ OpenChamber server                                                  │
│ • ~/.config/openchamber/settings.json (projects, labels, colors,    │
│   sidebar sections, display prefs, activeProjectId, lastDirectory)  │
│ • sessions-directories.json (folders)                               │
│ • session-drafts.json + session-drafts/<id>/ (drafts)               │
│ • sessions-archive.json (legacy archive drain)                      │
│ • git worktrees, project icons, scheduled tasks, source board, etc. │
└─────────────────────────────────────────────────────────────────────┘
                 ▲
                 │ /api/* do host OpenChamber
┌─────────────────────────────────────────────────────────────────────┐
│ Cliente (web/desktop/mobile)                                        │
│ • zustand stores em memória                                         │
│ • localStorage: preferências, colapsos, abas, caches de topologia   │
│ • row model: árvore, seções, busca, Show more, menus                │
└─────────────────────────────────────────────────────────────────────┘
```

A regra central: **dados duráveis e compartilhados ficam no servidor; preferências de superfície e estado efêmero ficam no cliente**. O sidebar só desenha o resultado de um `row model` único, que recebe listas já filtradas e ordenadas.

### 1.2 Princípios que explicam quase todas as decisões

1. **Global-first, não diretório-first.** A lista global de sessões é a fonte completa; stores por diretório só completam o que falta. `useGlobalSessionsStore.loadSessions` busca todas as sessões de uma vez, pagina por cursor e divide por arquivamento no cliente (`OC/stores/useGlobalSessionsStore.ts:760-849`, `OC/stores/globalSessions.ts:44-121`). Diretórios são usados para bootstrap e refresh local, não para descobrir a lista inteira.
2. **Nunca tratar falha de leitura como lista vazia.** Um fetch global falho mantém o snapshot anterior e marca `status: 'error'`; só um resultado completo vira autoridade (`OC/stores/useGlobalSessionsStore.ts:814-837`). O mesmo vale para pastas, drafts, projetos e ícones.
3. **Uma autoridade por campo.** Título, `time.archived`, `time.idle/viewed` e `metadata` pertencem ao OpenCode. Pastas, drafts e worktrees pertencem ao servidor OpenChamber. Abas, colapsos e largura pertencem ao cliente.
4. **Identidade por caminho normalizado + runtime.** Sessões e projetos são identificados por `(runtimeKey, diretório normalizado, sessionId)`; o mesmo `ses_...` pode existir em dois diretórios. Ver `getPinnedSessionKey` (`OC/lib/sessionPins.ts:12-18`), `useProjectsStore` (`OC/stores/useProjectsStore.ts:99-113`) e `useSessionFoldersStore` (`OC/stores/useSessionFoldersStore.ts:93`).
5. **Estado vivo deriva de canais vivos, não do histórico.** `busy`, permissões e perguntas vêm de índices globais/eventos; `archived`, `unread` e `outcome` vêm do registro da sessão, porque precisam sobreviver a restart (`OC/components/session/sidebar/DOCUMENTATION.md:51-55`, `OC/sync/session-unread.ts:13-30`).
6. **Otimista com rollback explícito.** Pin, work state, view e rename atualizam a UI antes da resposta; falha restaura o valor anterior (`OC/stores/useSessionPinnedStore.ts:109-114`, `OC/sync/session-unread.ts:47-76`).
7. **Migração e versão fazem parte do contrato.** Cada store persistido tem `version` e `migrate`; campos removidos são apagados, campos novos ganham default na hidratação (`OC/stores/useSessionDisplayStore.ts:101-149,209-232`).
8. **Preferências separadas por superfície quando a tela muda.** `perSurface: true` no registro de settings para `sidebarViewMode`, `sidebarGrouping`, `sidebarDetailedZoneRows`, `sidebarHiddenSections`; a mesma pessoa pode querer uma coisa no desktop e outra no telefone (`OC/lib/settings/registry.ts:393-411`).
9. **O row model é neutro de modo.** Normal, busca, projeto, timeline, Chats, pastas e runs usam o mesmo modelo de linhas; só mudam os blocos de entrada (`OC/components/session/sidebar/sessionSidebarRowModel.ts:53-129,483+`).
10. **Divulgação progressiva.** Cada projeto abre com 7 sessões, cada zona com 3 e mais 7 por clique; abas fixadas não gastam o limite (`OC/components/session/sidebar/DOCUMENTATION.md:518-540`, `OC/components/session/sidebar/projects/SessionGroupSection.tsx:370-371,924-941`).
11. **Cor carrega estado, não decoração.** Rodando = info; terminou e não lido = success; pedido pendente = warning; pin e marcas não-estado = muted (`OC/components/session/sidebar/DOCUMENTATION.md:57-79`).
12. **Acessibilidade por teclado é parte do desenho.** Todo hover revelado também aparece em `:focus-visible`; menus navegam por setas; Enter/Espaço abrem; Shift faz range selection; Shift sobre arquivar vira excluir (`OC/components/session/sidebar/DOCUMENTATION.md:634-651`, `OC/components/session/sidebar/sessions/SessionNodeItem.tsx:252-299`).

### 1.3 O que isso significa para o RustChamber

O RustChamber atual tem uma sidebar de **um único workspace e uma única seção “Chats”**, sem projetos agrupados, pastas, pins, arquivo, abas, seleção múltipla ou busca global (`RC/rchamber-ui/src/sidebar.rs:61-226`). O backend lista apenas as sessões do diretório atual e apenas raízes não arquivadas (`RC/rchamber-api/src/client.rs:273-299`). Portanto, não é uma questão de “adicionar menus”: é preciso antes criar o **modelo global de sessões/projetos** e a **camada de persistência local**, e só depois redesenhar a sidebar.

A boa notícia é que o RustChamber já tem a arquitetura certa para isso: `Action`/`BackendEvent` com ator Tokio (`RC/rchamber-app/src/backend.rs:23-155`), `AppModel` central (`RC/rchamber-app/src/state.rs:237-309`) e um sistema de temas/tokens próximo do OpenChamber (`RC/rchamber-ui/src/theme.rs:31-59`). O trabalho é, sobretudo, de modelo e UI, não de infraestrutura.

### 1.4 Estado atual do RustChamber: real, parcial e ausente

**Real e funcionando:**

- Lista de sessões do diretório atual, com título, modelo, custo, tokens e tempo (`RC/rchamber-app/src/state.rs:846-867`; `RC/rchamber-ui/src/sidebar.rs:805-976`).
- Criar, renomear, arquivar e excluir sessão via OpenCode (`RC/rchamber-app/src/backend.rs:388-414`; `RC/rchamber-ui/src/sidebar.rs:1318-1370`).
- Busca local por título/diretório, aplicada no Enter (`RC/rchamber-app/src/state.rs:846-867`; `RC/rchamber-ui/src/sidebar.rs:457-564`).
- Menu de workspaces com a lista de `GET /api/project` (`RC/rchamber-ui/src/sidebar.rs:622-674`; `RC/rchamber-app/src/backend.rs:264-267`).
- Drafts por `session_id` em `drafts.json` (`RC/rchamber-ui/src/persist.rs:146-181`).
- Tema, densidade, tipografia, largura da sidebar e animações (`RC/rchamber-ui/src/theme.rs:31-59`; `RC/rchamber-ui/src/app.rs:33-37`).
- Terminal, arquivos, changes e git já integrados (`RC/rchamber-ui/src/surfaces/*`).

**Parcial / só aparência:**

- “In progress” é derivado de `busy_any`, não do estado `openchamber.work` (`RC/rchamber-ui/src/sidebar.rs:88-96,1028-1037`).
- “Detailed rows” muda altura e mostra modelo/tempo, mas não a segunda linha de branch/projeto/PR (`sidebar.rs:914-932`).
- O menu de display só tem “New session” e “Detailed rows” reais; o resto são ações de conexão/configuração (`sidebar.rs:339-404`).
- O seletor de workspace lista projetos, mas não há registry de projetos, ordem, ícone, cor, colapso ou “Chat” (`sidebar.rs:622-674`).
- O rename é um modal, não inline (`RC/rchamber-ui/src/app.rs:747-804`).
- `sidebar_projects_closed` e `sidebar_chats_closed` vivem só no `UiState`/temp data, não persistem (`RC/rchamber-ui/src/sidebar.rs:99-102,129-151`; `RC/rchamber-ui/src/app.rs:305-349`).

**Morto / enganoso:**

- `AppModel.unread` nunca é preenchido: o mapa é criado vazio e só sofre `remove` ao ativar (`RC/rchamber-app/src/state.rs:263,299,352`); o check verde nunca aparece de forma correta.
- A árvore de subsessões existe no desenho, mas `list_sessions` pede `parentID=null`, então os filhos não chegam (`RC/rchamber-api/src/client.rs:273-299`; `RC/rchamber-ui/src/sidebar.rs:1181-1210`).
- O estado “show all” é uma única chave temporária no `Context`, não por grupo e não persistida (`sidebar.rs:99-102,211`).
- O menu “More” e o menu de display não correspondem aos do OpenChamber; não são placeholders de features reais.

**Ausente por completo:**

- Lista global de sessões e arquivadas; agrupamento por projeto/tempo; seções; caption; pins; pastas; worktrees; abas; seleção múltipla; marcar tudo como lido; busca global por ID/branch/projeto; renomear com IA; exportar Markdown; mover sessão/pasta/worktree; abrir no painel lateral/Mini Chat; issues/PRs; tarefas agendadas; estatísticas; catálogo de integrações; diálogo de adicionar projeto com lista/clone/importação; “fechar projeto” e “arquivar todas”.

---

## 2. Inventário de UI

### 2.1 Top bar

O topo é uma faixa de 40 px com quatro controles, sem título e sem botão “nova sessão” dentro da sidebar. O “Nova sessão” do screenshot fica na titlebar/header da janela, à esquerda do seletor de sidebar; é outro componente, não esta faixa.

| Elemento | Comportamento | Estado | Fonte |
| --- | --- | --- | --- |
| Lupa | Abre/fecha o campo de busca; quando aberto, o botão fica com fundo de seleção. Fecha ao clicar fora. | `isSessionSearchOpen` local do React; não persiste. | `OC/components/session/sidebar/shell/SidebarHeader.tsx:300-336,409-430`; `OC/components/session/sidebar/shell/useSessionSearchEffects.ts:47-61` |
| Pasta+ | Abre o diálogo “Adicionar projeto”. | Nenhum estado persistido no botão. | `OC/components/session/sidebar/shell/SidebarHeader.tsx:324-336` |
| Equalizador (display) | Abre o menu de visualização. | `sidebar_menu` no RustChamber; no OpenChamber é um `DropdownMenu` controlado. | `OC/components/session/sidebar/shell/SidebarHeader.tsx:338-361` |
| “...” (More) | Abre o menu de páginas: Overview, Issues e PRs, Tarefas agendadas, Arquivo, Estatísticas, Catálogo de integrações, páginas de extensões. | Cada página tem um booleano no `useUIStore`; o item mostra check quando aberto. | `OC/components/session/sidebar/shell/SidebarHeader.tsx:242-260,365-406` |

Detalhes visuais:

- Container: `px-2.5 py-1`, toolbar `min-h-8 gap-1.5`, primeiro glifo recuado 3 px para alinhar com os controles da titlebar (`OC/components/session/sidebar/shell/SidebarHeader.tsx:300-308`).
- Botões de 24 px, ícone de 16 px, `headerActionButtonClass`; ícone idle em `muted-foreground`, hover em `foreground` (`SidebarHeader.tsx:60-61,308-336`).
- O botão de busca aberto usa `bg-interactive-selection text-interactive-selection-foreground`, o mesmo token da linha selecionada (`SidebarHeader.tsx:60,314`).

No RustChamber, o equivalente está em `RC/rchamber-ui/src/sidebar.rs:266-310` (header), `318-337` (`action`), `339-404` (display) e `406-455` (more). A busca é um campo inline permanente em `457-564` e usa `⌘F` no tooltip (`sidebar.rs:287`), enquanto o OpenChamber usa `⌘K, L` (`OC/lib/shortcuts/config.ts:131-137`). O botão “More” já existe, mas o conteúdo é outro (Open folder, Reconnect, Settings, Keyboard shortcuts).

### 2.2 Seções, caption e árvore

Ordem padrão dos blocos (`OC/components/session/sidebar/sessionSidebarRowModel.ts:61`):

```
Chats
Pinned
In work
Projects
```

Cada bloco é uma seção do row model (`SessionSidebarBlock`, `sessionSidebarRowModel.ts:53-59`). O usuário pode reordenar, ocultar e criar seções; “Projects” é sempre mostrado (`OC/components/session/sidebar/sections/DOCUMENTATION.md:3-9,77`). “Chats” é uma raiz gerenciada para sessões fora de qualquer projeto, com pastas próprias, e nunca oferece ações de worktree (`DOCUMENTATION.md:103-117`).

- **Caption “Projects”**: aparece só quando há algo listado acima dos projetos e nunca no modo de um projeto só. É uma palavra muted, com chevron depois; clicar nela colapsa todos os projetos (`OC/components/session/sidebar/DOCUMENTATION.md:528-540`; `sessionSidebarRowModel.ts:66,1121-1140`).
- **Cabeçalho de projeto**: ícone de 14 px, label em `typography-ui-section` (15 px) e peso médio; o ícone vira chevron no hover. Sem fundo de hover na faixa inteira; só a linha da sessão recebe fill (`OC/components/session/sidebar/projects/sortableItems.tsx:41-111,302-410`; `OC/components/session/sidebar/DOCUMENTATION.md:544-548`).
- **Cabeçalho de worktree/espaço**: é uma “work row”, não um header comum; o ícone de branch fica no slot da árvore, o título é o nome da task ou o branch, e o estado vivo fica no fim (`OC/components/session/sidebar/DOCUMENTATION.md:353-389`; `OC/components/session/sidebar/works/WorktreeWorkRow.tsx`).
- **Cabeçalho de pasta**: linha de árvore com chevron, ícone e nome; pode conter subpastas (modelo legado) e mantém o estado de colapso próprio (`OC/components/session/sidebar/projects/SessionGroupSection.tsx:640-782`).
- **Sessões**: linha de 32 px (uma linha) ou 52 px (duas linhas). A uma linha tem `py-[5px]`; a duas linhas, `py-1.5` (`OC/components/session/sidebar/sessions/SessionNodeItem.tsx:1769-1785`; `sessionSidebarRowModel.ts:306-321`).
- **Árvore**: cada nível recua 12 px a partir de 8 px; o texto começa depois de um slot de 14 px + 6 px de gap. Guias verticais/horizontais são desenhadas em `SessionTreeGuides` (`OC/components/session/sidebar/sessions/sessionTreeGeometry.ts:1-23`; `SessionTreeGuides.tsx:7-37`).
- **Show more / Show less**: linha de 28 px, texto de 12 px muted, alinhada ao texto das sessões (`ps-[26px]`), hover vira `foreground` e sublinha (`SessionGroupSection.tsx:924-941`).
- **Vazio**: “No sessions yet / Create your first session to start coding.”; em busca, “No matching sessions / Try a different title, branch, folder, or path.” e link “Search the archive” (`OC/lib/i18n/messages/en.ts:653-658`; `OC/components/session/sidebar/shell/SidebarSearchEmptyState.tsx:16-31`).
- **Carregando por diretório**: spinner de 12 px + “Loading sessions...” dentro do grupo; falha mostra “Could not load...” com “Try again” e, se for permissão de SO, “Grant access” (`SessionGroupSection.tsx:811-838,895-923`).

No RustChamber, a seção única “Chats” está em `RC/rchamber-ui/src/sidebar.rs:129-208`; o label “In progress” é um `sub_label` (`1039-1058`); árvore em `1087-1210`; “Show more” em `1060-1085`; vazio em `780-803`.

### 2.3 Menu de visualização (display)

O menu é montado em grupos, não em uma lista plana (`SidebarHeader.tsx:162-235`). A ordem atual do código:

| Item | Tipo | Comportamento | Persistência | Atalho | Fonte |
| --- | --- | --- | --- | --- | --- |
| Personalizar seções… | Ação | Abre o gerenciador de seções ao lado da sidebar. | `sections`/`pinLists`/`hiddenSections` no settings do servidor. | — | `SidebarHeader.tsx:162-169`; `OC/components/session/sidebar/sections/SidebarSectionsManager.tsx` |
| Selecionar sessões / Sair da seleção | Toggle de modo | Entra no modo de seleção múltipla. | Efêmero (`useSessionMultiSelectStore`). | — | `SidebarHeader.tsx:170-175`; `OC/stores/useSessionMultiSelectStore.ts:49-71` |
| Sessões → Por projeto / Por data | Submenu de escolha | Agrupa por projeto ou em uma lista única por tempo. O ícone do submenu muda (`folders`/`time`). | `sidebarGrouping` (profile, perSurface) via `updateDesktopSettings`. | — | `SidebarHeader.tsx:180-189`; `OC/lib/settings/registry.ts:398` |
| Ordenar projetos → Manual, A→Z, Z→A, Mais recentes, Recentes | Submenu de escolha | Ordena os projetos. Em “Por data”, o grupo não aparece. | `sidebarProjectSortOrder` (profile). | — | `SidebarHeader.tsx:192-202`; `OC/lib/settings/registry.ts:400` |
| Exibir projetos → Todos os projetos / Um projeto | Submenu de escolha | “Um projeto” mostra só o projeto selecionado no seletor do topo. Em “Por data” fica escondido. | `sidebarProjectDisplayMode` (profile). | — | `SidebarHeader.tsx:203-209`; `OC/lib/settings/registry.ts:393` |
| Linhas detalhadas | Toggle | Duas linhas para works: título + branch/projeto/referência. | `sidebarDetailedZoneRows` (profile, perSurface). | — | `SidebarHeader.tsx:214-217`; `registry.ts:404` |
| Nomear worktrees pelo branch | Toggle | Nome do worktree = branch, task vai para a segunda linha. | `sidebarWorktreeNameByBranch` (profile). | — | `SidebarHeader.tsx:218-221`; `registry.ts:405` |
| Recolher tudo | Ação | Colapsa zonas, projetos, works e grupos; não colapsa pastas. Mantém “Projects” caption aberta e colapsa cada projeto. | `oc.sessions.projectCollapse`, `oc.sessions.groupCollapse`, `oc.sessions.activityCollapse`, `oc.sessions.workExpand`; e `sidebarCollapsed` por projeto no settings. | — | `OC/components/session/sidebar/projects/useSessionProjectViewState.ts:147-170` |
| Expandir tudo | Ação | Abre zonas, works, grupos e projetos; remove `PROJECTS_CAPTION_COLLAPSE_KEY`. | idem | — | `useSessionProjectViewState.ts:172-196` |
| Marcar tudo como lido | Ação (screenshot) | **Não existe neste checkout.** Ver 2.14. | — | — | busca em `OC/` não encontrou implementação |

Observações de interação:

- Escolher uma opção de visualização **não fecha o menu**, para permitir ajustar várias de uma vez; ações como Recolher/Expandir fecham (`SidebarHeader.tsx:122-125,143,153`).
- O submenu mostra a escolha atual à direita (`SidebarHeader.tsx:134-149`).
- O menu tem largura mínima de 200 px e separadores entre grupos (`SidebarHeader.tsx:353-359`).
- No telefone, o menu é outro: toggles de agrupamento, ordenação, “Show pinned”, “Show In work”, “Also show in work under projects”, “Detailed rows”, “Name worktrees by branch” e “Reorder projects” (`OC/components/session/sidebar/DOCUMENTATION.md:160-175`).

O gerenciador de seções abre como um popover ao lado da sidebar e carrega na primeira abertura. As linhas arrastam pela alça (não pelos controles), cada seção tem toggle de visibilidade, filtros em chips e o switch “Also show under projects”; o editor mostra só as condições que a seção já tem e oferece as demais em “Add condition”. Limites: `SIDEBAR_SECTIONS_MAX` seções do usuário e `SIDEBAR_PIN_LISTS_MAX` listas de pin (`OC/components/session/sidebar/sections/DOCUMENTATION.md:20-26,48-50,133-139`; `SidebarSectionsManager.tsx`).

No RustChamber, o menu atual (`sidebar.rs:339-404`) tem “New session”, “Detailed rows”, “Reconnect”, “Settings”, “Keyboard shortcuts”. Não há submenus, agrupamento, ordenação, seções nem colapso global.

### 2.4 Menu “...”

Itens atuais, na ordem (`SidebarHeader.tsx:242-260`):

| Item | Ícone | O que abre | Condição | Atalho |
| --- | --- | --- | --- | --- |
| Overview | `compass-3` | Página de overview de trabalhos em andamento. | `showSourceBoard && overviewAvailable` | — |
| Issues e PRs | `git-pull-request` | Source board (issues/PRs de GitHub, GitLab, Gitea, Forgejo, Gerrit, Linear etc.). | `showSourceBoard` | `⌘K, B` (`toggle_source_board`) |
| Tarefas agendadas | `calendar-schedule` | Diálogo de tarefas agendadas. | sempre | — |
| Arquivo | `archive` | Página de sessões arquivadas. | sempre | — |
| Estatísticas | `bar-chart` | Página de usage stats. | `showStats` | — |
| Catálogo de integrações | `store-2` | Catálogo de skills/plugins/serviços. | fora do modo enterprise | — |
| Páginas de extensões | ícone do guest | Páginas registradas por extensões. | `guestPages.length > 0` | — |

Cada item é um **toggle**: check quando a página está aberta; clicar de novo fecha (`SidebarHeader.tsx:384-389`). O menu rola verticalmente em janelas curtas (`382`). O atalho do source board é mostrado à direita (`388`).

No RustChamber, `more_menu` (`sidebar.rs:406-455`) só tem “Open folder…”, “Reconnect”, “Settings”, “Keyboard shortcuts”. As páginas não existem.

### 2.5 Projeto: header, menu, “+” e seletor

#### 2.5.1 Header

Estrutura: `ContextMenu` em volta de um header `sticky`, com o bloco inteiro arrastável (`sortableItems.tsx:303-342`). O header tem:

- ícone (14 px) ou imagem customizada (14 px, raio 3 px, fundo opcional);
- label 15 px medium;
- indicador de pasta ausente (`alert` warning) quando o diretório não existe;
- indicador de ação de diretório (`DirectoryActionIndicator`);
- botão de “novo worktree” (ícone branch) e menu “...” no hover; em touch ficam sempre visíveis (`sortableItems.tsx:277-300,418-516`).

O ícone do projeto **vira chevron** no hover/focus; é o mesmo padrão dos headers de worktree/espaço (`sortableItems.tsx:57-75`; `DOCUMENTATION.md:794-810`).

#### 2.5.2 Menu de contexto do projeto

Renderizado tanto pelo botão “...” quanto pelo clique direito (`sortableItems.tsx:208-255,311-333`):

| Item | Ícone | Condição | Fonte |
| --- | --- | --- | --- |
| Nova sessão | `add` | projeto não-repo ou `!hideDirectoryControls` | `sortableItems.tsx:210-215` |
| Gerenciar worktrees | `git-branch` | repo, desktop/web | `216-221` |
| Espaços isolados | `instance` | repo, feature ligada, não VS Code | `222-227` |
| Issues e PRs | `git-pull-request` | repo, source board disponível | `228-233` |
| Mostrar no Finder / Explorer / gerenciador de arquivos | `folder-received` | sempre que `onOpenFolder` existe | `234-239`; label via `getRevealLabelKey()` |
| Editar | `pencil-ai` | sempre | `240-243` |
| Arquivar todas as sessões | `archive` | sempre | `244-249` |
| Fechar projeto | `close`, destructive | sempre | `250-253` |

O screenshot mostra “Nova sessão, Mostrar no Finder, Editar, Arquivar todas as sessões, Fechar projeto” — é a variante sem os itens de repo/worktree/espaços. O comportamento é o mesmo.

**Arquivar todas** é confirmado e **não arquiva** sessões pinned, rodando ou em “In work”; também não arquiva descendentes de sessões que ficam (`SessionProjectScroller.tsx:209-233`). A confirmação diz quantas sessões irão e que pinned/running/In work ficam (`OC/lib/i18n/messages/en.ts:3884-3888`).

**Fechar projeto** remove o projeto da lista, mas **não apaga sessões**. Também limpa as pastas de todos os escopos do projeto (root, arquivo e worktrees), remove o mapa de worktrees e para espaços isolados; se era o ativo, muda para o próximo ou para o diretório “sem projeto” (`useProjectsStore.ts:792-840`).

#### 2.5.3 “+”

O “+” do header do projeto abre uma nova sessão em rascunho no diretório do projeto (ou worktree). Em worktree, o botão de novo chat vira “Archive” enquanto Shift está pressionado; em sessão, o arquivar vira excluir (`DOCUMENTATION.md:409-419`; `SessionNodeItem.tsx:252-299`).

No projeto raiz, “Nova sessão” chama `openNewSessionDraft({ selectedProjectId, directoryOverride, target })`; a sessão só nasce no envio (`SessionGroupSection.tsx:701-710`). Isso é diferente do RustChamber, que cria a sessão no servidor imediatamente (`RC/rchamber-ui/src/app.rs:566-577`).

#### 2.5.4 Seletor de projeto no topo

No modo “Um projeto”, o header do projeto vira um dropdown com a lista de projetos e check no atual (`sortableItems.tsx:343-370`; `SessionProjectScroller.tsx:195-205`). O dropdown usa `ProjectHeaderIdentity` e fecha ao escolher.

O screenshot mostra “Buscar projetos…, Chat, stellares-platform, Rust Chamber ✓”. Esse campo de busca com “Chat” é o **project picker do composer**, não o seletor do header: `OC/components/chat/composer/ui/DraftTargetSelectors.tsx:388-434` (campo `searchProjects`) e `702` (picker). O seletor do header neste checkout não tem campo de busca nem opção “Chat”. Para fidelidade ao screenshot, o seletor do topo deve ganhar busca e a entrada “Chat”; a lógica já existe no composer e pode ser extraída.

### 2.6 Sessão: linha, estados, hover e menu

#### 2.6.1 Anatomia da linha

Uma linha de sessão tem:

```
[slot 14px] [título .........................................] [meta cluster]
             ↑ chevron quando tem filhos; senão ícone de estado
```

- O slot fica à esquerda do título; mostra o chevron quando a sessão tem filhos e o estado vivo quando não tem (`SessionNodeItem.tsx:880-912`; `DOCUMENTATION.md:57-79`).
- O título é `typography-ui-label` (13.5 px). Em repouso usa `oc-title-rest` (mistura 50% foreground + muted); rodando usa foreground; não lido ou pedido pendente usa `font-semibold text-foreground`; selecionado usa `text-interactive-selection-foreground` (`SessionNodeItem.tsx:1646-1653`).
- O cluster direito mostra, na ordem: goal, PR/issue, badges de permissão/pergunta, indicador de diretório, pin, “looks done”, ponto de estado e duração/tempo (`SessionTimelineRowBody.tsx:130-160`).
- Não há data na linha: a lista é ordenada por recência; o tooltip carrega a data. A duração de um turno aparece enquanto roda e até ser lida (`DOCUMENTATION.md:190-195`; `SessionTimelineRowBody.tsx:150-160`).
- Tooltip da linha: título, hora, projeto, branch/espaço, refs (PR/issue), recap do último turno (`SessionNodeItem.tsx:1850-1905`).
- Altura: 32 px uma linha, 52 px duas linhas. Padding horizontal: 6 px de base + 12 px por nível; texto +20 px (`sessionTreeGeometry.ts:1-23`).
- Fundo: `hover:bg-interactive-hover/60`; selecionado `bg-interactive-selection/70` + texto de seleção; em seleção múltipla, `ring-1 ring-inset ring-border` (`SessionNodeItem.tsx:1770-1785`).

#### 2.6.2 Estados visuais

| Estado | Ícone/cor | Regra | Fonte |
| --- | --- | --- | --- |
| Rodando (turno próprio) | `circle` info (azul), com spinner opcional | `session.time.busy > idle` ou status vivo | `DOCUMENTATION.md:57-79`; `OC/sync/global-session-status.ts` |
| Rodando por subagente | `robot` info | turno mantido aberto por subagente | idem |
| Rodando por comando | `terminal` info | turno mantido por shell em background | idem |
| Aguardando PR | `git-pull-request` info | `sync/pr-watch.ts` | idem |
| Terminou, não lido | `checkbox-circle` success (verde) | `idle > viewed`, sessão top-level não arquivada | `OC/sync/session-unread.ts:13-30` |
| Permissão pendente | `shield` warning | request de permissão | `SessionNodeItem.tsx:1580-1590` |
| Pergunta/form pendente | `question` warning | form aguardando resposta | idem |
| Pin | ícone de pin muted | metadata `openchamber.pinned` | `OC/lib/sessionPins.ts:20-37` |
| “Looks done” | check muted | sugestão do Jev | `SessionNodeItem.tsx:1568-1578` |
| Goal | glyph de goal | metadata de goal | `SessionNodeItem.tsx:584-592` |
| Worktree/branch | ícone branch, tint do PR | work row | `DOCUMENTATION.md:353-375` |
| Espaço isolado | ícone `instance` | work row de espaço | idem |

O RustChamber atual usa um anel azul para rodando, `Shield` para permissão e um `Check` verde para “unread” (`RC/rchamber-ui/src/sidebar.rs:1288-1300`). O check verde está próximo, mas o correto é `checkbox-circle` e a regra `idle > viewed`; hoje o mapa `unread` nunca é preenchido (`RC/rchamber-app/src/state.rs:263,352`; busca por `unread.insert` não encontra nada).

#### 2.6.3 Ações no hover

Ações aparecem no fim da linha, sobre a meta, sem empurrar o layout (`oc-actions-mask`; `DOCUMENTATION.md:634-641`). Ordem:

1. Track / Done (olho / check) — só quando a linha “owns work”.
2. Arquivar rápido (ícone archive); com Shift vira excluir.
3. Abrir no editor (quando disponível).
4. Menu “...” (sempre).

Em touch, Track/Done vão para o menu e as ações ficam sempre visíveis (`DOCUMENTATION.md:638-644`).

#### 2.6.4 Menu de contexto da sessão

Fonte principal: `SessionNodeItem.tsx:1214-1548`. Itens e condições:

| Item | Ícone | Condição | Ação/atalho | Fonte |
| --- | --- | --- | --- | --- |
| Renomear | `pencil-ai` | sempre | Rename inline; atalho `⌘K, R` quando é a sessão atual | `1228-1242` |
| Renomear com IA | gerado por `SessionAiRenameMenuItem` | small model disponível, não VS Code, com turnos concluídos | chama `/api/small-model/generate` | `1243`; `OC/components/session/SessionAiRenameMenuItem.tsx`; `OC/lib/sessionTitle.ts:128-144` |
| Copiar ID da sessão | `file-copy` | sempre | clipboard + toast | `1244-1249`; `useSessionActions.ts:136-146` |
| Fixar em → Pinned / listas / nova lista / Desfixar | pin | sempre (top-level) | grava metadata; submenu | `1250-1265`; `OC/components/session/sidebar/sections/PinToMenu.tsx` |
| Acompanhar como em andamento / Marcar como concluída | `eye` / `check` | quando `canTrackWork` | metadata `openchamber.work.state` | `1266-1273`; `OC/lib/sessionWorkMetadata.ts` |
| Manter aquecida / Parar de aquecer | `fire` / `fire-fill` | quando `canKeepWarm` | `/api/session-warming/sessions/:id` | `1274-1281` |
| Exportar Markdown | `download` | sempre | gera `.md` e salva/baixa | `1282-1287`; `OC/lib/exportSession.ts` |
| Ações de extensões | ícone do guest | guests registrados | ação do guest | `1288-1293` |
| Mover para worktree → worktrees existentes / Novo worktree… | `folder-shared` | não-subtask, não arquivada, não VS Code, com diretório | move árvore inteira; worktree atual desabilitado | `1294-1397`; `OC/components/session/sidebar/sessionWorktreeMenu.ts` |
| Mover sessão… | `folder-received` | não-subtask, não arquivada, não VS Code, com diretório | abre `MoveSessionDialog` (projetos + escolher pasta) | `1398-1407`; `OC/components/session/MoveSessionDialog.tsx:33-95` |
| Abrir visão geral do run | merge | quando é multi-run | abre overview | `1408-1415` |
| Mover para pasta → pastas / nova pasta / remover | `folder` | diretório, não arquivada, autoridade de pasta | grava `sessions-directories.json` | `1417-1496` |
| Abrir no painel lateral | `chat-4` | não VS Code, com diretório | cria aba no painel | `1498-1509` |
| Abrir na janela Mini Chat | `window` | Electron | invoke desktop | `1511-1522` |
| Arquivar | `inbox-archive` | não arquivada | PATCH `archived: true`; atalho `⌘K, Backspace` | `1524-1532` |
| Restaurar | `inbox-unarchive` | arquivada | PATCH `archived: false` | `1534-1540` |
| Excluir | `delete-bin`, destructive | sempre | DELETE; confirma se configurado; Shift no quick archive vira delete | `1542-1547`; `useSessionActions.ts:148-195` |

Regras de confirmação:

- Se `showDeletionDialog` está ligado, arquivar/excluir abre um diálogo que informa quantos descendentes serão afetados. O snapshot de descendentes é congelado ao abrir (`useSessionActions.ts:171-195`).
- Excluir uma sessão arquivada é hard delete; arquivar uma ativa é soft archive (`useSessionActions.ts:148-168`; `sessionSubtreeActions.ts`).
- O menu de contexto por clique direito usa os mesmos itens, com `ContextMenu` (`SessionNodeItem.tsx:1698-1745`).

### 2.7 Pastas

- Pastas são **por escopo de projeto**: root do projeto, cada worktree e o escopo de arquivados (`__archived__:<root>`). O `scopeKey` é o diretório normalizado; o `ownerKey` é o projectId (`OC/components/session/sidebar/sessions/sessionFolderIdentity.ts:7-50`; `OC/lib/sessionFolders/folderScopes.ts:1-2`).
- Modelo: `{ id, name, sessionIds, createdAt, parentId? }`, com subpastas suportadas no modelo mas **não mais criáveis pela UI** (`OC/stores/useSessionFoldersStore.ts:20-29`; `SessionGroupSection.tsx:946-949`).
- A criação, renomeação e exclusão são otimistas e enfileiradas para o servidor (`useSessionFoldersStore.ts:631-699`).
- Arrastar uma sessão para uma pasta usa DnD; arrastar um worktree para pasta não é oferecido (`DOCUMENTATION.md:693-702`).
- A pasta aparece como linha de árvore, com chevron, e as sessões dentro dela ficam um nível mais fundo; subpastas são exibidas como “Pai / Filho” no mesmo nível, sem mais indentação (`SessionGroupSection.tsx:757-782`).
- Excluir pasta com sessões: no arquivo, “Delete sessions”; fora dele, confirma com contagem de subpastas/sessões (`SessionGroupSection.tsx:659-682`; `ConfirmDialogs.tsx`).

### 2.8 Seleção múltipla

- `useSessionMultiSelectStore` guarda `enabled`, `selectedIds`, `scopeKey`, `anchorId`, `anchorRowKey`; tudo efêmero (`OC/stores/useSessionMultiSelectStore.ts:5-11,49-174`).
- Um clique seleciona a linha e todos os descendentes; Shift seleciona um range baseado na ordem visual (`toggleSelected`, `setRange`; linhas 73-146).
- O range usa `SessionRowOrderEntry` com `descendantRange` para incluir subárvores sem expandir tudo (`setRange` 115-146).
- A barra de ações em massa tem: contagem, mover para pasta, arquivar, excluir, restaurar (no arquivo) e sair (`OC/components/session/sidebar/folders/BulkActionBar.tsx:48-160`).
- O RustChamber não tem nada disso; a estrutura de `UiState` atual não prevê seleção (`RC/rchamber-ui/src/app.rs:305-349`).

### 2.9 Busca de sessões

- Abre pela lupa ou `⌘K, L`; fecha com Esc; clicar fora fecha e limpa (`useSessionSearchEffects.ts:34-61`; `SessionSidebar.tsx:95-107`).
- O input é **draft local**: digitar não invalida a árvore. Só Enter comita a query; limpar o campo com texto vazio comita `''`; Esc limpa ou fecha (`OC/components/session/SessionSearchInput.tsx:29-98`).
- A dica inferior mostra “Press Enter to search” ou “X matches” e “Esc to clear”; quando a seção é editável, oferece “Save as section” (`SidebarHeader.tsx:418-427`; `SessionSearchInput.tsx:88-96`).
- Regra de match: um `ses_...` exato busca por ID; senão cada palavra da query precisa começar uma palavra no título, branch, nome de pasta ou path (`OC/components/session/sidebar/recent/activitySections.ts:13-22`; `OC/lib/search/fuzzySearch.ts:184-220`).
- A busca lista cada work **uma vez**, na primeira seção que o contém; works saem dos projetos enquanto a busca está aplicada (`OC/components/session/sidebar/sections/DOCUMENTATION.md:43-47`).
- Para um root cujo subsession casa, o root permanece como contexto e abre para o match (`activitySections.ts:24-37`; `DOCUMENTATION.md:624-629`).
- O RustChamber busca no cliente, apenas título/diretório, e só aplica ao pressionar Enter (`RC/rchamber-app/src/state.rs:846-867`; `RC/rchamber-ui/src/sidebar.rs:457-564`). Não busca branch, projeto, pasta nem ID; não tem “salvar como seção” nem “buscar no arquivo”.

### 2.10 Adicionar projeto, clonar e importar do OpenCode

#### 2.10.1 O que o código atual faz

O diálogo único `DirectoryExplorerDialog` (`OC/components/session/DirectoryExplorerDialog.tsx`):

- Campo de caminho com ícone de pasta, fonte mono, botão de submit embutido (“Add project”, “Add selected”, “Already added”, “Clone & add”, “Create & add”) (`841-903`; labels em `OC/lib/i18n/messages/en.ts:2181-2225`).
- Lista de diretórios com linha “..” para subir, ícone de pasta, nome, badge “Added” quando já é projeto, checkbox de seleção múltipla e botão “+” de quick add (`905-1019`).
- Teclado: ↑/↓ navegam; Enter abre a pasta destacada; `⌘/Ctrl+Enter` adiciona o caminho; Backspace com campo vazio fecha; Espaço seleciona quando está navegando (`765-804`).
- Rodapé: dicas “Navigate / Select / Add”, “Open in Finder” e botão “Clone repository”/“Add local project” (`1029-1073`).
- Modo clone: campo “Repository URL (HTTPS or SSH)”, identidade Git, destino “Clone in”, botão “Clone & add”; valida se a URL e a identidade combinam; usa `runGitClone` e mostra progresso/recuperação (`233-283,415-540,610-713,848-868`).
- Importação do OpenCode: bloco no topo da lista que lê `GET /api/project`, filtra diretórios disponíveis e não configurados, e oferece “Add projects from OpenCode” / “Don't suggest again” (`OC/components/session/project-import/OpenCodeProjectImport.tsx:18-92`; `OC/lib/i18n/messages/en.ts:57-60`).
- O RustChamber usa `rfd::FileDialog::new().pick_folder()` direto (`RC/rchamber-ui/src/sidebar.rs:293-298`), sem lista, sem recentes, sem clone, sem importação, sem badge “Adicionado”.

#### 2.10.2 O que o screenshot mostra

O screenshot mostra um modal grande com uma **rail lateral**: Início, lipecode, Stellare, Clonar repositório, Do OpenCode; à direita o caminho `~/Projects/lipecode/`, refresh, lista de pastas, “Rust Chamber — Adicionado”, rodapé ↑↓ Navegar / ↵ Abrir / Abrir no Finder / “Adicionar 'lipecode'”. O modal de clone tem URL, “Clonar em”, lista com “Adicionado” e “Clonar e adicionar”.

Esse layout de rail **não existe neste checkout**. O código atual é uma tela única com o caminho, a lista e o toggle de clone. Para reproduzir o screenshot, a rail pode ser construída como um `egui::SidePanel` interno do modal, com:
- “Início” = home do usuário;
- recentes = projetos já registrados + últimos diretórios;
- “Clonar repositório” = modo clone;
- “Do OpenCode” = bloco de importação.

A funcionalidade existe; a apresentação é que difere. O estudo recomenda implementar a rail como melhoria visual, não como novo backend.

#### 2.10.3 Fluxo de adicionar por pasta

1. Abre o diálogo (pasta+).
2. Resolve o home com `GET /api/fs/home` (ou `getFilesystemHome` no desktop) (`DirectoryExplorerDialog.tsx:167-183,265-275`).
3. Lista o diretório com `GET /api/fs/list` (`OCS/lib/fs/routes.js:1986`).
4. Usuário navega ou digita o caminho; “Add” chama `addProject(path)` (`useProjectsStore.ts:643-706`).
5. `addProject` normaliza, deduplica por path, cria `{ id, path, label, labelSource, color, addedAt, lastOpenedAt }`, insere, ativa, persiste no settings e dispara descoberta de ícone (`useProjectsStore.ts:669-705`).
6. O projeto aparece na sidebar; a pasta é adicionada ao store de diretórios conhecidos e o ícone é carregado de `GET /api/projects/:id/icon` (`useProjectsStore.ts:984-1077`; `OCS/lib/projects/routes.js`).
7. Se a pasta já é projeto, o item mostra “Added” e o botão fica “Already added”.

#### 2.10.4 Fluxo de clonar

1. Alterna para o modo clone.
2. Informa URL; o nome da pasta é derivado do último segmento sem `.git` (`DirectoryExplorerDialog.tsx:147-165`).
3. Escolhe destino (campo “Clone in”) e identidade Git; o botão só habilita se a identidade alcança o tipo de URL (`415-540`).
4. `runGitClone` cria o checkout; se já existe pasta, o destino é tratado como checkout (`610-713`).
5. Ao terminar, `addProject` é chamado; se o clone exigir setup de acesso, mostra aviso e não clona de novo (`696-704`; i18n `2198`).

### 2.11 Abas de sessões abertas

- `useSessionTabsStore` guarda `tabIds`, `previewTabId`, `scope`, `sets` e `pinOnOpenIds`. Uma aba “preview” é substituída pela próxima sessão aberta; double-click, drag ou envio de mensagem fixam a aba (`OC/stores/useSessionTabsStore.ts:7-26,96-217`).
- Limite: 10 abas fixadas; 40 escopos armazenados; o escopo global é `global` (`67-70,86-94`).
- Persistência: `localStorage` via zustand `persist` com chave `session-tabs-store`, versão 1; a versão 0 vira o escopo global (`218-225`).
- O RustChamber não tem abas; `RC/rchamber-ui/src/app.rs` não tem noção de working set. É uma funcionalidade independente da sidebar e pode ficar para depois (P5).

### 2.12 Estados vazios, carregando e erro

| Situação | UI | Fonte |
| --- | --- | --- |
| Sem sessões | “No sessions yet” + “Create your first session to start coding.” | `OC/lib/i18n/messages/en.ts:653-655` |
| Busca sem resultado | “No matching sessions” + dica + “Search the archive” | `SidebarSearchEmptyState.tsx:16-31` |
| Projeto vazio | “Start a session” link no fim do grupo | `SessionGroupSection.tsx:895-904` |
| Carregando diretório | spinner + “Loading sessions...” | `SessionGroupSection.tsx:905-913` |
| Falha de bootstrap | “Could not load...” + “Try again” / “Grant access” | `SessionGroupSection.tsx:811-838` |
| Falha global | mantém lista anterior; `status='error'`; não zera | `useGlobalSessionsStore.ts:814-837` |
| Pasta de projeto ausente | ícone `alert` warning no header | `sortableItems.tsx:398-406` |
| Projeto sem worktree | header normal; worktree row só aparece se houver | `worktreeIndex.ts` |

### 2.13 Atalhos de teclado relevantes

Fonte: `OC/lib/shortcuts/config.ts`.

| Ação | Atalho padrão | Fonte |
| --- | --- | --- |
| Abrir busca de sessões | `⌘K, L` | `131-137` |
| Nova sessão | `⌘N` | `55-60` |
| Nova sessão em worktree | `⌘⇧N` | `138-144` |
| Renomear sessão atual | `⌘K, R` | `90-96` |
| Arquivar sessão atual | `⌘K, Backspace` | `97-103` |
| Sessão anterior/próxima | `⌘⌥←` / `⌘⌥→` | `61-74` |
| Voltar/avançar histórico | `⌘[` / `⌘]` | `75-89` |
| Abrir/fechar sidebar | `⌘B` (painéis) | `220-225` |
| Fechar aba | `⌥W` | `111-116` |
| Abrir project picker do draft | `⌘K, P` | `117-123` |
| Abrir worktree picker do draft | `⌘K, G` | `124-130` |
| Buscar histórico do prompt | `⌘↑` | `41-47` |
| Focar input | `⌘I` | `35-40` |

No RustChamber, os atalhos existentes são `⌘K`/`⌘P` (palette), `⌘B` (sidebar), `⌘J` (terminal), `⌘⇧M` (modelo), `⌘⌥←/→` (ciclo de sessão), `⌘N`, `⌘,` e Esc (`RC/rchamber-ui/src/app.rs:639-719`). Não há `⌘K, L`, `⌘K, R`, `⌘K, Backspace` nem navegação por menus com setas no padrão OpenChamber (há `menu_nav_focus`, mas só nos menus já existentes).

### 2.14 Fluxos detalhados

#### Fluxo A — criar projeto por pasta

```
pasta+ → DirectoryExplorerDialog → GET /api/fs/home + /api/fs/list
      → escolhe/ digita path → addProject()
      → normaliza, deduplica, label = último segmento, cor automática
      → persiste em settings.json + localStorage
      → ativa projeto (setDirectory + opencodeClient.setDirectory)
      → discoverProjectIcon (GET/PUT /api/projects/:id/icon)
      → sidebar mostra o projeto e as sessões do diretório
```

Citações: `SidebarHeader.tsx:324-336`; `DirectoryExplorerDialog.tsx:217-275,720-804,841-903`; `useProjectsStore.ts:643-706,984-1077`; `useDirectoryStore.ts:282-309`.

#### Fluxo B — clonar repositório

```
Clone mode → URL + destino + identidade
           → runGitClone (POST /api/git/network-operations ou /api/fs/clone)
           → addProject(destino)
           → se setupRequired, aviso e não repete
```

Citações: `DirectoryExplorerDialog.tsx:233-283,415-540,610-713`; `OCS/lib/fs/routes.js:875`; `OCS/lib/git/routes.js` (network operations).

#### Fluxo C — fechar projeto

```
project menu → Close project
             → removeProject(id)
             → remove de projects + manualOrder
             → limpa pastas de root/arquivo/worktrees
             → remove worktrees do mapa
             → para espaços isolados do projeto
             → ativa próximo projeto ou goToNoProjectDirectory()
```

Citações: `sortableItems.tsx:250-253`; `useProjectsStore.ts:792-840`; `useDirectoryStore.ts:371-397`.

#### Fluxo D — arquivar / excluir

```
Archive:
  menu/hover → handleDeleteSession(session,{archivedBucket})
  → se showDeletionDialog: confirma com descendantCount
  → archiveSession(s) → PATCH /api/session/:id { archived:true }
  → global store move ativo→arquivado; aba fecha; In work remove
Delete:
  hardDelete:true → DELETE /api/session/:id
  → remove do global store + fecha abas/descendentes
```

Citações: `useSessionActions.ts:148-195`; `OC/sync/session-actions.ts:1599-1791`; `OC/sync/session-archive-batch.ts:1-90`; `OP/packages/protocol/src/groups/session.ts:301-320`.

#### Fluxo E — renomear com IA

```
menu → SessionAiRenameMenuItem → useSessionAiRename.prepare()
  → GET /api/session/:id (no diretório)
  → carrega até 3 turnos concluídos (user + assistant final)
  → POST /api/small-model/generate { prompt, system, directory,
       preferredProviderID, preferredModelID, restrictToPreferredProvider }
  → valida título (1 linha, ≤100 chars)
  → relê a sessão e só salva se title/directory/archived/revert não mudaram
  → PATCH /api/session/:id { title }
```

Prompt: `OC/lib/sessionTitle.ts:20-27` (system prompt de 2-4 palavras, até 40 chars, sem verbos, sem paths) e `128-144` (body). Contexto: `use-session-ai-rename.ts:18-50`. Elegibilidade: `useSessionAiRenameAction.ts:10-49` (small model, não VS Code, turnos concluídos). O RustChamber **não pode** chamar `/api/small-model/generate` se fala direto com OpenCode; precisa usar `/api/experimental/generate/text` do OpenCode ou `/api/session/:id/generate` (ver 7.3).

#### Fluxo F — exportar Markdown

```
menu → Export Markdown → carrega histórico + filhos
  → formatSessionAsMarkdown (título, data, mensagens, subagentes)
  → saveDesktopMarkdownFile no desktop ou download no web
  → toast com “Reveal” quando salvou em disco
```

Citações: `SessionNodeItem.tsx:736-760`; `OC/lib/exportSession.ts:100-140`; i18n `sessions.sidebar.session.export.*`.

#### Fluxo G — mover para pasta / worktree / sessão

- **Pasta**: grava `sessionIds` no arquivo da pasta; a sessão sai de outra pasta do mesmo projeto antes (`SessionNodeItem.tsx:1417-1496`; `useSessionFoldersStore.ts:701-732`).
- **Worktree**: usa `requestSessionTreeMove` com kind `existing` ou `quick`; move a árvore inteira (root + descendentes) e carrega mudanças não commitadas se a origem estiver suja (`SessionNodeItem.tsx:1294-1397`; `OC/components/session/sidebar/sessionWorktreeMenu.ts`).
- **Sessão**: `MoveSessionDialog` lista projetos e “Choose folder…”; move a árvore, não o disco (`MoveSessionDialog.tsx:33-95`; `OP/packages/protocol/src/groups/session.ts:324-334`).

#### Fluxo H — fixar, marcar como lido, acompanhar

- **Fixar**: `PinToMenu` → `setSessionPinList` → `setSessionPinned` → `PATCH /api/session/:id` metadata `openchamber.pinned { at, list? }`; UI otimista com pending overlay (`OC/stores/useSessionPinnedStore.ts:76-92,109-114`; `OC/lib/sessionPins.ts:20-37`; `OC/sync/session-actions.ts:1294-1308`).
- **Marcar como lido**: abrir a sessão chama `markSessionViewed` → `markSessionUnreadSeen` grava `viewed = idle` localmente, chama `POST /api/session/:id/view { idle }`; o evento `session.viewed` limpa em todos os clientes (`OC/sync/session-unread.ts:47-76`; `OP/packages/protocol/src/groups/session.ts:810-820`).
- **Marcar tudo como lido**: não há implementação. Proposta: iterar as sessões `isSessionUnread` e chamar o mesmo endpoint em paralelo limitado (2-4), aplicando watermark otimista e rollback por sessão. Não existe endpoint em lote no OpenCode; um lote no servidor OpenChamber exigiria nova rota.
- **Acompanhar em andamento**: metadata `openchamber.work.state = open|done`; enviar mensagem abre, o usuário fecha; “Looks done” é sugestão do Jev (`OCS/lib/session-work/state.js:1-70`; `OC/lib/sessionWorkMetadata.ts:5-80`; `OC/components/session/sidebar/DOCUMENTATION.md:597-632`).

#### Fluxo I — painel lateral / Mini Chat / Issues e PRs / tarefas

- **Painel lateral**: cria uma aba de sessão no context panel, com o diretório da sessão atual (`SessionNodeItem.tsx:1498-1509`).
- **Mini Chat**: só Electron; `invokeDesktop('desktop_open_session_mini_chat_window', { sessionId, directory })` (`1511-1522`).
- **Issues e PRs**: source board do OpenChamber; agrega trackers e abre item; não existe no OpenCode puro. Rotas e providers ficam em `OCS/lib/{github,gitlab,gitea,forgejo,gerrit,linear}/` e `OC/components/sourceBoard/`.
- **Tarefas agendadas**: runtime do servidor OpenChamber; persiste por projeto em `project-config`, descobre loops em `.agents/loops`, cria sessões com metadata `openchamber.kind='scheduled'`; tem escopo especial “Chats” (`OCS/lib/scheduled-tasks/DOCUMENTATION.md:1-60`).
- **Estatísticas**: `GET /api/experimental/session/stats` no OpenCode (`OP/packages/protocol/src/groups/session.ts:146-165`), com página no OpenChamber.
- **Integrações**: catálogo do servidor OpenChamber; instala skills/plugins; não é transportável para o OpenCode puro.

### 2.15 Screenshots ↔ código

| Screenshot | O que o código tem | Divergência/ação |
| --- | --- | --- |
| Sidebar dark completa | Seções Chats/In work/Projects, árvore, caption, Show more (`sessionSidebarRowModel.ts`; `SessionGroupSection.tsx`) | Reproduzir tokens e alturas de 8; nomes localizados (“Conversas”, “Em andamento”) |
| Popover “Pesquisar sessões ⌘ + K, L” | Tooltip do botão de busca + atalho (`SidebarHeader.tsx:309-322`; `config.ts:131-137`) | `⌘K,L`, não `⌘F` |
| Popover “Adicionar projeto” | Botão pasta+ abre o diálogo (`SidebarHeader.tsx:324-336`) | Mesmo comportamento |
| Popover “Modo de visualização” | Display menu (`SidebarHeader.tsx:338-361`) | Ordem de 2.3 |
| Menu “...” | More menu (`SidebarHeader.tsx:365-406`) | Inclui Overview no código; screenshot não mostra |
| Busca aberta | Input + dicas (`SessionSearchInput.tsx`) | Mesmo |
| Modal “Adicionar projeto” com rail | Tela única + caminho/lista/clone/import (`DirectoryExplorerDialog.tsx`) | **Rail não existe**; construir no P5 |
| Modal “Clonar repositório” | Modo clone do mesmo diálogo (`233-713`) | Mesmo comportamento; layout pode virar rail |
| Topo “Nova sessão” | Titlebar/header da janela | Não é a faixa da sidebar |
| Display menu expandido | Itens de 2.3 | “Marcar tudo como lido” **não existe** |
| Menu de projeto | `sortableItems.tsx:208-255` | Itens de repo/worktree podem aparecer; screenshot é a variante mínima |
| Menu de sessão | `SessionNodeItem.tsx:1214-1548` | “Fixar em” é “Pin to” no código; todos os demais existem |
| Dropdown de projeto com busca | Seletor do header é simples (`sortableItems.tsx:343-370`); busca + “Chat” é o picker do composer (`DraftTargetSelectors.tsx:388-434,702`) | Extrair o picker do composer para o topo |

---

## 3. Modelo de dados e fluxo

### 3.1 Entidades

#### Projeto (cliente OpenChamber)

```ts
type ProjectEntry = {
  id: string;                 // derivado do path normalizado
  path: string;               // caminho absoluto normalizado
  label?: string;
  labelSource?: 'manual' | 'auto';
  icon?: string;              // nome de ícone do mapa
  iconImage?: { mime; updatedAt; source: 'custom' | 'auto' } | null;
  iconBackground?: string | null;
  color?: string;             // preset de cor
  defaultAgent?: string;
  defaultModel?: string;      // provider/model
  defaultVariant?: string;
  addedAt?: number;
  lastOpenedAt?: number;
  sidebarCollapsed?: boolean;
};
```

Fonte: `OC/lib/api/types.ts` (ProjectEntry), sanitização em `useProjectsStore.ts:250-338`. O projeto do OpenCode é outro tipo, com `id`, `canonical`, `name?`, `icon?`, `vcs?`, `time`, `sandboxes` (`OP/packages/schema/src/project.ts:28-45`).

#### Sessão

```ts
type Session = {
  id: string;
  parentID?: string;
  projectID: string;
  directory?: string;     // location.directory no wire
  project?: { id?: string; worktree?: string } | null;
  title?: string;
  agent?: string;
  model?: ModelRef;
  time: { created; updated; busy?; idle?; viewed?; archived? };
  metadata?: Record<string, unknown>;
  outcome?: 'succeeded' | 'failed' | 'interrupted';
  fork?: { sessionID; boundary };
  cost; tokens; revert?;
};
```

Fonte: `OP/packages/schema/src/session.ts:32-63`; mapeamento do cliente em `OC/lib/opencode/model`. No RustChamber, `Session` já tem quase todos os campos, mas `directory` é `location.directory` e `metadata` é `Option<Value>` (`RC/rchamber-api/src/models.rs:56-100`).

#### Árvore e grupos (row model)

```ts
type SessionNode = {
  session: Session;
  children: SessionNode[];
};

type SessionGroup = {
  key: string;
  label: string;
  directory: string | null;
  isMain: boolean;
  isArchivedBucket?: boolean;
  branch?: string | null;
  sessions: SessionNode[];
  folderScopes?: { scopeKey; directory }[];
  emptyMessage?: string;
};
```

Fonte: `OC/components/session/sidebar/types.ts`; construção em `useSessionGrouping.ts` e `sessionCollection.ts`.

#### Pasta, pin, draft, worktree

```ts
type SessionFolder = { id; name; sessionIds: string[]; createdAt; parentId?: string | null };
type SessionDraft = {
  id; directory; projectId: string | null; text; mentions: string[];
  attachments: { id; filename; mimeType; size; kind: 'data' | 'path'; path? }[];
  createdAt; updatedAt;
};
type WorktreeMetadata = {
  path; branch?; label?; name?; worktreeRoot; worktreeStatus: 'ready' | 'missing';
  headState: 'branch' | 'detached' | 'unborn'; source; projectDirectory;
};
```

Fontes: `OC/stores/useSessionFoldersStore.ts:20-29`; `OC/stores/useSessionDraftsStore.ts:15-39`; `OC/types/worktree.ts` e `OC/lib/worktrees/worktreeManager.ts`.

### 3.2 Como o OpenChamber enxerga todos os projetos e sessões

O ponto que mais confunde: **o OpenCode não é “um servidor por diretório”**. O servidor é um só; cada request pode carregar `directory` (header `x-opencode-directory` e/ou query) para escopar a operação. O cliente OpenChamber usa:

- **Lista global**: `listSessionsPage({ global: true })` → `GET /api/session` sem `directory`, com `order`, `limit`, `cursor`; isso retorna sessões de todos os diretórios (`OC/lib/opencode/client.ts:1084-1104`; `OC/stores/globalSessions.ts:79-84`).
- **Refresh por diretório**: `refreshSessionsForDirectories` chama `listGlobalSessionPages` com `directory` para cada diretório conhecido (root + worktrees), com concorrência 2 (`useGlobalSessionsStore.ts:286-330,851-938`).
- **Store por diretório**: cada diretório tem seu child store para mensagens/status; a lista global é a união autoritativa (`OC/sync/child-store.ts`; `OC/stores/useGlobalSessionsStore.ts:45-72`).
- **Projetos**: `useProjectsStore` é a lista registrada pelo usuário (settings), não `GET /api/project`. O `GET /api/project` é usado para importar projetos do OpenCode e para o fallback de ownership (`useProjectsStore.ts:1116-1159`; `OC/components/session/project-import/OpenCodeProjectImport.tsx:40-65`).
- **Sessões sem projeto**: vão para “Chats” (raiz gerenciada `~/.config/openchamber/chats`) ou, se o diretório não é registrado nem Chat, ficam fora de In work/Pinned/search até serem admitidas; o snapshot global ainda as mantém para limpeza/linhagem (`DOCUMENTATION.md:83-96,597-622`; `OC/lib/chatDirectories.ts`).

Para o RustChamber, a tradução direta é:

```
GET /api/session?order=desc&limit=500
  (sem directory, sem parentID, sem archived)
  → pagina por cursor.next
  → divide active/archived por time.archived
  → agrupa por projectID/location.directory
  → resolve ownership por projeto registrado
  → monta árvore por parentID
```

Isso substitui o atual `list_sessions(100)` escopado e com `parentID=null` (`RC/rchamber-api/src/client.rs:273-299`), que hoje esconde subsessões e arquivadas.

### 3.3 Ownership: como uma sessão encontra seu projeto

Algoritmo do OpenChamber (`OC/components/session/sidebar/sessions/sessionOwnership.ts:1-180`):

1. Indexa cada projeto registrado pelo path normalizado; o dono é `{ projectId, projectRoot, scopeDirectory: root, kind: 'project' }`.
2. Indexa cada worktree sob seu projeto; `kind: 'worktree'`. Worktree vence o root por prefixo mais longo.
3. Indexa espaços isolados; `kind: 'space'`.
4. Para cada sessão, tenta o diretório da sessão (`location.directory`) e, se não houver, `project.worktree`.
5. O match é por prefixo de path mais longo; o dono escolhido é o mais específico. Em empate, `kind: 'project'` vence.
6. Se nada resolve, tenta o projeto autoritativo do OpenCode **somente** quando o worktree canônico dele mapeia para um projeto registrado; o projeto `global` do OpenCode é excluído (`DOCUMENTATION.md:83-96`).
7. Sessões sem dono não entram em In work/Pinned/search; ficam no snapshot global para limpeza e linhagem (`DOCUMENTATION.md:597-622`).

O RustChamber pode simplificar: como o app é local e os projetos são registrados pelo usuário, indexar roots + worktrees locais e usar prefixo; sessões fora de qualquer root vão para “Sem projeto” (ou Chats). Não tentar o fallback `projectID` do OpenCode na primeira versão.

### 3.4 Subsessões (parentID) e linhagem

- A árvore é montada por `parentID`: roots são as sessões sem pai; filhos ficam sob o pai. O row model verifica a presença do pai no cache global ativo + arquivado antes de renderizar; um filho cujo pai é conhecido nunca vira root independente (`DOCUMENTATION.md:236-240`).
- A linhagem de exibição não é só `parentID`: também considera `originalSessionID` (auto-review e `/btw`) e `fork.sessionID`; forks comuns continuam roots (`DOCUMENTATION.md:331-351`). Para a primeira versão do RustChamber, usar apenas `parentID`; marcar review/btw/fork como “root” é aceitável e explícito.
- Subagentes/workflow agents (`metadata.openchamber.kind === 'workflow-agent'`) ficam na árvore mas não geram linhas; o run acima mostra a atividade (`DOCUMENTATION.md:238`). O RustChamber pode simplesmente mostrar todos os filhos; se virar ruído, adicionar o filtro depois.
- Sessões arquivadas continuam na árvore; um pai arquivado não promove filho ativo a root (`DOCUMENTATION.md:236-240`).
- O `GlobalSessionStructure` mantém índices incrementais: `activeRootIds`, `activeChildrenByParentId`, `activeIdsByDirectory`, para não reconstruir tudo a cada evento (`OC/stores/globalSessionStructure.ts:4-9,115-243`). É um bom modelo para o `AppModel`: `HashMap<parent_id, Vec<id>>` + `HashMap<directory, Vec<id>>`.

### 3.5 Worktrees

- O OpenChamber descobre worktrees via `GET /api/git/worktrees` e mantém `availableWorktreesByProject: Map<projectPath, WorktreeMetadata[]>` no `useSessionUIStore` (`OC/lib/worktrees/worktreeManager.ts`; `OC/sync/session-ui-store.ts:491-493`; `OC/components/session/SessionSidebar.tsx:176-230`).
- A topologia é persistida em `oc.worktreeMap.v2` por runtime, até 8 runtimes (`OC/sync/worktree-topology-cache.ts:4-6,58-95`).
- Um worktree é um “work” como uma sessão: linha própria, abre/fecha, tem estado vivo, PR tint, menu (Track/Done, New chat, Rename task, Move to folder, Delete worktree) e entra em Show more (`DOCUMENTATION.md:353-389`).
- Sessões dentro de worktrees são agrupadas pelo worktree root; `findPrefixWorktreeEntry` usa o prefixo mais longo (`DOCUMENTATION.md:704-714`).
- Mover sessão para worktree usa `session.move` do OpenCode; o worktree em si é criado por `POST /api/git/worktrees` (`OP/packages/protocol/src/groups/session.ts:324-334`; `OCS/lib/git/routes.js:1747`).
- O RustChamber tem `rchamber-git` com status/diff/branches, mas **não tem** `worktree list/add/remove` nem clone (`RC/rchamber-git/src/repo.rs:103-345`). Precisa adicionar.

### 3.6 Ordenação, agrupamento e Show more

- Ordem de ciclo de vida: `sessionOrderIndex` é calculado a partir da ordem de atividade (mais recente primeiro), com pinned primeiro; `compareSessionsByLifecycleOrder` é o comparador (`OC/sync/session-ordering.ts`; `SessionGroupSection.tsx:302-311`).
- Em projeto: roots e works são ordenados por essa ordem; worktrees entram na posição da sessão mais recente (`DOCUMENTATION.md:119-133`).
- Em zona (Pinned/In work/timeline): mistura projetos; a linha compacta não tem ícone de projeto, e a detalhada tem duas linhas (`DOCUMENTATION.md:556-582`).
- “Show more”: por grupo, `visibleSessionCountByGroup` é estado local do componente, incremento 7; ao colapsar, o limite é resetado (`SessionProjectCollection.tsx:202-240`); o row model recebe `visibleCountByContainer` (`sessionSidebarRowModel.ts:160-243,896`).
- Chats: revelação inicial 3 + 7; pinned não gasta limite; projetos abrem com no máximo 7 (`DOCUMENTATION.md:518-524`).
- O RustChamber usa `MAX_VISIBLE_ROOTS = 8` e um único estado `show_all` (`RC/rchamber-ui/src/sidebar.rs:59,99-102,191-204`); não é por grupo e não reseta ao colapsar.

### 3.7 Seções como filtros

- Uma seção do usuário é um filtro sobre “works” (sessão + subárvore, ou worktree/espaço). Não é uma segunda lista de sessões (`OC/components/session/sidebar/sections/DOCUMENTATION.md:103-109`).
- Filtros disponíveis: tipo (sessão/worktree/espaço), status (rodando, ocioso, falhou, aguardando, não lido, concluído), atividade (“Hoje”, “Últimos 7 dias”, “Ocioso 3+ dias”), projeto, branch, agente, modelo, goal, PR/issue, texto (“Título ou branch”), “Scheduled run”, pin list (`DOCUMENTATION.md:51-70`).
- Uma seção pode listar o mesmo work em várias; o switch “Also show under projects” decide se ele também fica no projeto (`DOCUMENTATION.md:28-47,111-124`).
- “Projects” lista o que nenhuma seção/grupo tomou (`DOCUMENTATION.md:34-37`).
- O RustChamber não tem esse conceito. Para a primeira versão, implementar apenas as seções embutidas (Chats/Pinned/In work/Projects) e deixar seções customizadas para P6.

### 3.8 Diagrama de um clique em sessão

```
click row
  → handleSessionSelect(id, directory)
      → fecha superfícies (closeMainSurfaces)
      → se mobile, fecha switcher
      → se já é a atual: reseta busca e retorna
      → setCurrentSession(id, directory)
          → resolve directory (global record → worktree map → hint)
          → checa admission; se não admitida, tenta admitir
          → fecha draft
          → grava runtime memory + last session
          → setDirectory(directory)
          → setActiveProjectIdOnly(project)
          → setSingleProjectId(project)
          → opencodeClient.setDirectory(directory)
          → markSessionViewed(id)
      → onSessionSelected
      → resetSessionSearch
```

Citações: `OC/components/session/sidebar/sessions/useSessionActions.ts:77-100`; `OC/sync/session-ui-store.ts:1511-1640`; `OC/sync/session-unread.ts:47-76`.

---

## 4. Mapa completo de memória e persistência

### 4.1 Autoridades por dado

| Dado | Autoridade | Cópia no cliente | Ciclo de vida | Fonte |
| --- | --- | --- | --- | --- |
| Sessões (título, parentID, location, time) | OpenCode | `useGlobalSessionsStore` + `entityById` + snapshot | Carrega no boot; eventos SSE; 45 s de polling global; refresh por diretório | `OC/stores/useGlobalSessionsStore.ts:1-1056`; `DOCUMENTATION.md:43-55` |
| Arquivamento | OpenCode (`time.archived`) + arquivo legado OpenChamber | idem | PATCH `archived`; arquivo legado drenado | `OC/sync/session-actions.ts:1680-1791`; `OCS/lib/openchamber-sessions/archive-store.js:1-20` |
| Não lido | OpenCode (`time.idle`/`time.viewed`) | idem | `POST /api/session/:id/view`; evento `session.viewed` | `OC/sync/session-unread.ts:5-30,47-76` |
| Pins | OpenCode metadata `openchamber.pinned` | `useSessionPinnedStore` derivado + overlay de pending | PATCH metadata; pins legados migrados | `OC/lib/sessionPins.ts:5-60`; `OC/stores/useSessionPinnedStore.ts:1-129` |
| In work / done / goal | OpenCode metadata `openchamber.work` | derivado do global | PATCH metadata; Jev sugere, usuário fecha | `OCS/lib/session-work/state.js:1-70`; `OC/lib/sessionWorkMetadata.ts` |
| Projetos (lista, labels, cores, ícones, defaults, colapso) | OpenChamber settings | `useProjectsStore` + localStorage | `PUT /api/config/settings`; sync events; debounce de 700 ms no colapso | `OC/stores/useProjectsStore.ts:1-1219`; `OCS/index.js:393` |
| Ordem manual de projetos | OpenChamber settings + localStorage `:manualOrder` | `manualProjectOrder` | `reorderProjects` | `useProjectsStore.ts:353-413,1079-1101` |
| Pastas | OpenChamber server `sessions-directories.json` | `useSessionFoldersStore` + localStorage por runtime | PATCH `/api/session-folders`; debounce 150 ms; fila offline | `OC/stores/useSessionFoldersStore.ts:56-762`; `OCS/lib/session-folders/routes.js:140-255` |
| Drafts | OpenChamber server `session-drafts.json` | `useSessionDraftsStore` | PUT/DELETE `/api/session-drafts/:id`; 200 drafts; 25 MB por anexo | `OC/stores/useSessionDraftsStore.ts:1-212`; `OCS/lib/session-drafts/routes.js:14-239` |
| Seções / pin lists | OpenChamber settings (profile) | `session-display-mode` localStorage + store | `sidebarSections`, `sidebarPinLists`, `sidebarHiddenSections` | `OC/stores/useSessionDisplayStore.ts:161-252`; `OC/lib/settings/registry.ts:409-411` |
| Display mode / sort / show projects / detailed / branch names | OpenChamber settings (profile) + localStorage | `useSessionDisplayStore` | `updateDesktopSettings` em cada mudança | `SidebarHeader.tsx:180-221`; `registry.ts:393-405` |
| Colapso de projetos | settings `projects[].sidebarCollapsed` + localStorage | `useSessionProjectViewState` | debounce 700 ms para settings; imediato no localStorage | `useSessionProjectViewState.ts:9-12,79-103` |
| Colapso de zonas/grupos/works | localStorage apenas | `useSessionProjectViewState` | imediato; `workExpand` guarda default + toggles | `useSessionProjectViewState.ts:9-31,115-140` |
| Colapso de pastas | localStorage `oc.sessions.folderCollapse` por runtime | `useSessionFoldersStore` | imediato + debounce 300 ms | `useSessionFoldersStore.ts:62,231-278` |
| Abas abertas | localStorage `session-tabs-store` | `useSessionTabsStore` | zustand persist; 10 pinned / 40 escopos | `OC/stores/useSessionTabsStore.ts:96-227` |
| Escopo de abas | localStorage | idem | `global` ou project/worktree/space | `OC/lib/sessionTabScope.ts` |
| Última sessão | localStorage `oc.lastSession.v1` | `last-session-cache` | por runtime; 8 entradas | `OC/sync/last-session-cache.ts:8-130` |
| Última sessão por projeto | localStorage `oc.sessions.activeSessionByProject` | `SessionSidebar` | por projeto; usada ao trocar de projeto | `OC/components/session/SessionSidebar.tsx:61,117-137` |
| Último diretório | settings `lastDirectory` + localStorage | `useDirectoryStore` | `setDirectory` grava ambos | `useDirectoryStore.ts:297-298,399-475`; `registry.ts:238` |
| Projeto ativo | settings `activeProjectId` (bootstrap-only) + localStorage | `useProjectsStore` | cada janela mantém seu ativo após bootstrap | `useProjectsStore.ts:1116-1159`; `registry.ts:245` |
| Largura da sidebar | localStorage `ui-store` | `useUIStore` | por dispositivo; não vai ao servidor | `useUIStore.ts:198,1537,3816-3818` |
| Worktrees (topologia) | OpenChamber server (git) | `useSessionUIStore` + `oc.worktreeMap.v2` | descoberta por projeto; persiste por runtime | `OC/sync/worktree-topology-cache.ts:4-95` |
| Ícones de projeto | OpenChamber server `projects/<id>/…` + settings | `useProjectsStore` | PUT/DELETE/discover | `useProjectsStore.ts:964-1077` |
| Tarefas agendadas | OpenChamber server project config | UI store | runtime + rotas | `OCS/lib/scheduled-tasks/DOCUMENTATION.md` |
| Project knowledge (notes/todos/plans) | OpenChamber server `/api/project-context/:projectId` | `useProjectContextStore` | leitura/escrita otimista, write chain por projeto | `OC/stores/useProjectContextStore.ts:1-120` |
| Session warming | OpenChamber server `/api/session-warming` | `useSessionWarmingStore` | snapshot com revision; não persiste no cliente | `OC/stores/useSessionWarmingStore.ts:1-131` |
| Multi-seleção / busca / show more | memória do componente | — | efêmero | `useSessionMultiSelectStore.ts`; `SessionSidebar.tsx:95-107`; `SessionProjectCollection.tsx:202-240` |

### 4.2 Chaves de localStorage (exatas)

| Chave | Formato | Versão/migração | Quem escreve | Fonte |
| --- | --- | --- | --- | --- |
| `session-display-mode` | JSON do store | `version: 12`; migrações 1→12 | `useSessionDisplayStore` | `useSessionDisplayStore.ts:101-149,209-232` |
| `session-tabs-store` | `{ tabIds, previewTabId, scope, sets }` | `version: 1`; v0 → global | `useSessionTabsStore` | `useSessionTabsStore.ts:218-225` |
| `ui-store` | JSON do `useUIStore` | `version: 26`; migrações grandes | `useUIStore` | `useUIStore.ts:3543-3545,3814-3977` |
| `projects-store` | devtools name only (não persiste; a persistência é manual) | — | `useProjectsStore` | `useProjectsStore.ts:588,1204` |
| `projects` / `projects:<origin>` | array de ProjectEntry | legado + namespace por origin | `useProjectsStore` cache | `useProjectsStore.ts:90-120,390-397` |
| `activeProjectId` / `activeProjectId:<origin>` | string | legado + namespace | `useProjectsStore` | `useProjectsStore.ts:110-113,377-388` |
| `projects:<origin>:manualOrder` | array de ids | — | `useProjectsStore` | `useProjectsStore.ts:353-362,407-413` |
| `oc.sessions.projectCollapse` | array de ids | lido/escrito direto | `useSessionProjectViewState` | `useSessionProjectViewState.ts:9,60-62` |
| `oc.sessions.groupCollapse` | array de chaves | direto | idem | `useSessionProjectViewState.ts:10,63-65` |
| `oc.sessions.activityCollapse` | array de chaves | direto | idem | `useSessionProjectViewState.ts:11,66-68` |
| `oc.sessions.workExpand` | `{ expandedByDefault, toggled[] }` | schema zod | idem | `useSessionProjectViewState.ts:12-31` |
| `oc.sessions.folders.v2.index` | `{ version: 2, legacyClaimed, runtimes[] }` | v2 | `useSessionFoldersStore` | `useSessionFoldersStore.ts:63,87-112` |
| `oc.sessions.folders.v2:<runtime>` | `Record<scopeKey, SessionFolder[]>` | v2; legado migrado | idem | `useSessionFoldersStore.ts:93,191-229` |
| `oc.sessions.folderCollapse.v2:<runtime>` | array de ids | v2 | idem | `useSessionFoldersStore.ts:93,231-246` |
| `oc.sessions.folderChanges.v2:<runtime>` | array de FolderChange | v2; fila offline | idem | `useSessionFoldersStore.ts:67,146-168` |
| `oc.sessions.foldersShared.v2:<runtime>` | `1` | v2 | idem | `useSessionFoldersStore.ts:69,170-177` |
| `oc.sessions.folderDeletions.v2:<runtime>` | `Record<id, timestamp>` | v2; tombstones | idem | `useSessionFoldersStore.ts:65,134-143` |
| `oc.sessions.folders` / `oc.sessions.folderCollapse` | legado | migrado uma vez | idem | `useSessionFoldersStore.ts:60-62,114-128` |
| `oc.sessions.pinned.v2` | `{ version:2, sessions: Record<key,0> }` | v2; legado apagado | `useSessionPinnedStore` | `useSessionPinnedStore.ts:20-62` |
| `oc.sessions.pinned` | legado | removido | idem | `useSessionPinnedStore.ts:21,47` |
| `oc.sessions.expandedParents.v3` | array de chaves | v3 | `useExpandedParents` | `OC/components/session/sidebar/sessions/useExpandedParents.ts:5-34` |
| `oc.sessions.activeSessionByProject` | `Record<projectId, sessionId>` | zod | `SessionSidebar` | `SessionSidebar.tsx:61,117-137` |
| `oc.lastSession.v1` | `{ version:1, runtimes: Record<runtime,{sessionId,directory,updatedAt}> }` | v1; 8 runtimes | `last-session-cache` | `OC/sync/last-session-cache.ts:8-130` |
| `oc.worktreeMap.v2` | `{ version:2, legacyClaimed, runtimes: Record<runtime,{entries,updatedAt}> }` | v2; legado `oc.worktreeMap` | `worktree-topology-cache` | `OC/sync/worktree-topology-cache.ts:4-95` |
| `oc.global-sessions.v1.<hash>` | `{ version:1, runtimeKey, sessions[] }` | v1; limites de tamanho | `persist-cache` | `OC/sync/persist-cache.ts:28,33,389-430` |
| `oc.session-activity.v1` | mapa de starts de turno | v1 | `session-activity-timing` | `OC/sync/session-activity-timing.ts:116,267-331` |
| `oc.chatInput.lastDraftTarget` | alvo do último draft | — | composer | busca em `OC/stores` |
| `homeDirectory`, `lastDirectory` | strings | — | `useDirectoryStore` | `useDirectoryStore.ts:115-131,282-309` |
| `openchamber.settingsMirror.v2:<runtime>` | espelho do settings | v2; 5 runtimes | `persistence.ts` | `OC/lib/persistence.ts:62-119` |
| `openchamber.deviceSeeded.v1:<runtime>` | timestamp | v1 | `persistence.ts` | `OC/lib/persistence.ts:44-60` |

### 4.3 Arquivos do servidor OpenChamber

Root padrão: `~/.config/openchamber` (`OCS/index.js:344-352`); `OPENCHAMBER_DATA_DIR` sobrescreve; `OPENCHAMBER_CHATS_DIR` muda só a raiz de Chats (`OCS/index.js:352-360`).

| Arquivo | Conteúdo | Quem lê/escreve | Fonte |
| --- | --- | --- | --- |
| `settings.json` | Documento de settings (profile + instance + per-surface), incluindo `projects`, `activeProjectId`, `lastDirectory`, `sidebar*`, `sidebarSections`, `sidebarPinLists`, `openCodeProjectImportCompleted` | `/api/config/settings` GET/PUT; stores | `OCS/index.js:393`; `OCS/lib/opencode/routes.js:97-301`; `OC/lib/settings/registry.ts:238-245,393-411` |
| `sessions-directories.json` | `{ version:1, foldersMap, collapsedFolderIds, deletedFolderIds, updatedAt }` | `/api/session-folders` GET/POST/PATCH | `OCS/lib/session-folders/routes.js:140-255` |
| `session-drafts.json` | `{ version:1, drafts[] }`, até 200 | `/api/session-drafts` GET/PUT/DELETE | `OCS/lib/session-drafts/routes.js:14-239` |
| `session-drafts/<draftId>/<attachmentId>` | bytes de anexos | rota de attachment | idem |
| `sessions-archive.json` | `{ [sessionID]: archivedAt | null }` legado | drenagem para OpenCode | `OCS/lib/openchamber-sessions/archive-store.js:1-20` |
| `projects/<id>/icon.*` | ícone customizado | `/api/projects/:id/icon` | `OCS/lib/projects/routes.js` |
| `project-config` | tarefas agendadas, config por projeto | scheduled tasks | `OCS/lib/scheduled-tasks/DOCUMENTATION.md` |
| `git-worktree-bootstrap.json` | estado de bootstrap de worktree | git routes | `OCS/index.js:419` |
| `opencode-trust.json` | diretórios confiáveis | trust | `OCS/index.js:397` |
| `chats/` | raiz gerenciada de sessões sem projeto | chats scope | `OCS/index.js:352-360` |

### 4.4 O que fica no OpenCode

O OpenCode v2 usa SQLite, não mais a árvore `storage/session/...` do v1. O diretório de dados é `~/.local/share/opencode` (XDG), e o banco é `opencode.db` (ou `opencode-<channel>.db` em canais de dev), conforme `OP/packages/util/src/global-roots.ts:1-20` e `OP/packages/cli/src/database-path.ts:1-12`. A migração v1→v2 importa o storage legado (`OP/packages/core/src/database/v1-migration.bun.ts`).

Campos que o sidebar usa e que são autoridade do OpenCode:

- `title`
- `parentID`
- `location.directory` / `projectID`
- `time.{created,updated,busy,idle,viewed,archived}`
- `metadata` (pins, work, goal, links, review/btw)
- `outcome`
- `fork`

Endpoints correspondentes: `GET /api/session`, `PATCH /api/session/:id`, `POST /api/session/:id/view`, `POST /api/session/:id/move`, `DELETE /api/session/:id`, `POST /api/session` (`OP/packages/protocol/src/groups/session.ts:126-146,160-230,301-334,810-820`).

### 4.5 Settings: profile, instance e per-surface

O registro classifica cada campo (`OC/lib/settings/registry.ts`):

- `profile`: compartilhado por todas as instâncias do mesmo usuário (ex.: `sidebarGrouping`, `sidebarSections`, `sidebarProjectSortOrder`).
- `instance`: por servidor/instalação (ex.: `projects`, `activeProjectId`, `lastDirectory`, `terminalShell`).
- `perSurface`: grava em `fields[key].surfaces[<surface>]`; a leitura resolve superfície → base. Superfícies: `vscode`, `desktop`, `mobile`, `web` (`OC/lib/settings/surface.ts`; `DOCUMENTATION.md:20-23`).
- `adopt: 'bootstrap-only'`: só adota o valor do servidor no bootstrap; mudanças posteriores de outra janela não roubam o ponteiro local (`registry.ts:238,245`; `useProjectsStore.ts:1116-1159`).

O cliente mantém um espelho em `localStorage` para boot offline e para não piscar valores errados; o servidor continua sendo a autoridade (`OC/lib/persistence.ts:62-240`). O RustChamber não tem esse servidor; ele pode manter um único JSON local com a mesma separação lógica (ver 4.7).

### 4.6 Migrações e ciclo de vida

Exemplos que importam para a réplica:

- **Display store**: v1→v2 adiciona `projectSortOrder`; v2→v3 troca `recent` por `manual`; v4 remove `displayMode`; v6 remove `sessionGroupingMode`; v7 remove `stickyZoneHeaders`; v10 remove `showRecentSection`; v11 remove `worktreeSortOrder`; v12 liga duas linhas no telefone (`useSessionDisplayStore.ts:101-149`).
- **Folders**: legado sem namespace é reivindicado uma vez por runtime; tombstones de exclusão têm TTL; fila de mudanças sobrevive a reload (`useSessionFoldersStore.ts:56-177,544-606`).
- **Pins**: pins antigos locais são migrados para metadata quando o snapshot global fica `ready`; se falhar, ficam para a próxima carga (`OC/sync/session-pins.ts:70-120`).
- **Tabs**: v0 global vira `scope: 'global'`; ids desconhecidos são mantidos até confirmação de remoção (`useSessionTabsStore.ts:9-20,218-225`).
- **Projetos**: labels auto com title-case de versões antigas são revertidos para o nome da pasta; `labelSource` distingue manual de auto (`useProjectsStore.ts:179-195,281-290`).
- **Worktrees**: `oc.worktreeMap` legado é reivindicado uma vez e removido (`worktree-topology-cache.ts:58-95`).
- **Snapshots globais**: versão 1, limite de tamanho; se não couber, remove o snapshot antigo em vez de guardar um parcial (`persist-cache.ts:389-430`).

### 4.7 Como reproduzir no RustChamber

#### 4.7.1 Opções

| Opção | Vantagens | Desvantagens | Recomendação |
| --- | --- | --- | --- |
| JSON local (`sessions-projects.json`) | Simples, atômico, fácil de migrar, funciona offline, não depende de servidor | Não sincroniza entre máquinas; precisa de write queue | **Base principal** |
| Metadata do OpenCode (`PATCH /api/session/:id`) | Sincroniza entre clientes; sobrevive a troca de máquina; alinhado ao OpenChamber | Requer read-modify-write; OpenCode pode ter múltiplos escritores; metadata é opaca | Para pins/work/goal, se o servidor for compartilhado |
| `egui` storage (`eframe` persistence) | Zero código de arquivo | Não é inspecionável, não versiona, não serve para dados grandes, difícil de migrar | Só geometria efêmera de janela, nunca dados de sessão |
| Servidor OpenChamber (`/api/session-folders`, `/api/session-drafts`, `/api/config/settings`) | Sincroniza exatamente como o OpenChamber; reutiliza formato | Só existe se o usuário rodar OpenChamber; RustChamber fala direto com OpenCode | Opcional, como “modo compatível”, não como dependência |

#### 4.7.2 Desenho recomendado

Criar `RC/rchamber-app/src/workspace.rs` (ou `store.rs`) com:

```rust
#[derive(Serialize, Deserialize)]
pub struct WorkspaceStore {
    pub version: u32,                 // começa em 1
    pub projects: Vec<ProjectEntry>,  // labels, cor, ícone, defaults
    pub order: Vec<String>,           // ids na ordem manual
    pub active_project_id: Option<String>,
    pub last_session_id: Option<String>,
    pub last_session_by_project: HashMap<String, String>,
    pub sections: Vec<SidebarSection>,
    pub pin_lists: Vec<PinList>,
    pub hidden_sections: Vec<String>,
    pub folders: HashMap<String, Vec<SessionFolder>>, // scopeKey -> folders
    pub collapsed: CollapsedState,    // projects, groups, zones, folders, works
    pub tabs: TabState,               // opcional
    pub drafts: HashMap<String, String>, // manter o drafts.json atual
}
```

- Caminho: `directories::BaseDirs::config_dir()/rustchamber/workspace.json` no macOS/Linux/Windows (mesmo diretório do `settings.json` atual, `RC/rchamber-ui/src/persist.rs:141-144`).
- Escrita atômica: `write tmp` + `fs::rename`; debounce 300-500 ms; flush em `on_exit`/`pagehide` equivalente (`eframe::App::on_exit`).
- Uma fila de escrita única (`mpsc`) no backend ou no app; nunca escrever de forma concorrente.
- Migração: `version: 0`/ausente → aplicar defaults; `version < N` → funções puras de migração; nunca apagar dados sem backup `.bak`.
- Corrupção: ler falhou → manter o arquivo como `.corrupt-<timestamp>` e começar default; não sobrescrever silenciosamente (modelo do `archive-store.js:100-140`).
- Multi-instância: se o app abrir duas janelas, usar um lock ou última-escrita-vence com revisão; para dados de UI, aceitável; para projetos, usar revisão e recarregar.
- Separação: `settings.json` continua para tema, largura, densidade, endpoint, senha, workspace; `workspace.json` para o domínio de sessões/projetos. Não misturar (o `Settings` atual é pequeno e estável, `RC/rchamber-ui/src/persist.rs:8-40`).

#### 4.7.3 O que deve ir para o OpenCode

- **Pins e work state**: gravar em `metadata.openchamber.pinned` e `metadata.openchamber.work` via `PATCH /api/session/:id`. Isso permite que outro cliente (OpenChamber, TUI) veja o mesmo estado. Fazer read-modify-write: `GET` → merge → `PATCH` com o objeto completo. Aceitar que uma corrida com outro cliente pode perder uma chave; para um app local, é aceitável; se necessário, serializar por sessão.
- **Título**: `PATCH { title }`; após salvar, reler a sessão para confirmar (`OC/sync/session-actions.ts:2039-2069`).
- **Arquivar**: `PATCH { archived: true|false }`.
- **View**: `POST /api/session/:id/view { idle }` (watermark).
- **Mover**: `POST /api/session/:id/move { directory }`.
- **Não** gravar pastas/ordem/seções no OpenCode: não há contrato e o metadata deve ficar pequeno.

#### 4.7.4 O que deve ficar só local

- Ordem manual, seções customizadas, listas de pin, colapsos, abas, largura, última sessão, last session by project, “show more”, seleção, busca.
- Pastas: a princípio local. Se o usuário quer compartilhar com OpenChamber, implementar um adaptador que lê/escreve `/api/session-folders` quando o endpoint existir; o formato local deve ser idêntico (`{ foldersMap, collapsedFolderIds, updatedAt }`).
- Drafts: manter o `drafts.json` atual por sessão; para “novo rascunho de sessão” (antes de criar a sessão), criar um `new_session_drafts` com `{ id, directory, projectId, text, attachments, updatedAt }`, no mesmo formato do OpenChamber, mas local.

### 4.8 Mapa de chaves proposto para o RustChamber

| Chave local | Equivalente OpenChamber | Notas |
| --- | --- | --- |
| `workspace.projects` | `projects` no settings | inclui label/cor/ícone/defaults |
| `workspace.order` | `manualProjectOrder` + ordem de `projects` | uma única lista |
| `workspace.active_project` | `activeProjectId` | por janela? começar global |
| `workspace.last_session` | `oc.lastSession.v1` | por endpoint/runtimeKey |
| `workspace.last_session_by_project` | `oc.sessions.activeSessionByProject` | |
| `workspace.sections` | `sidebarSections` | ids, tipo, filtros, visibilidade |
| `workspace.pin_lists` | `sidebarPinLists` | |
| `workspace.hidden_sections` | `sidebarHiddenSections` | |
| `workspace.folders` | `foldersMap` | `scopeKey` = diretório normalizado |
| `workspace.collapsed.projects` | `oc.sessions.projectCollapse` | |
| `workspace.collapsed.groups` | `oc.sessions.groupCollapse` | |
| `workspace.collapsed.zones` | `oc.sessions.activityCollapse` | |
| `workspace.collapsed.folders` | `oc.sessions.folderCollapse` | |
| `workspace.expanded_works` | `oc.sessions.workExpand` | |
| `workspace.tabs` | `session-tabs-store` | opcional |
| `workspace.display` | `session-display-mode` | detailed, grouping, sort, show projects |
| `workspace.ui.sidebar_width` | `ui-store.sidebarWidth` | pode continuar em `settings.json` |
| `workspace.drafts` | `session-drafts.json` | manter `drafts.json` atual |

---

## 5. Tabela de funcionalidades

Legenda de status no RustChamber: **E** = existe, **P** = parcial, **F** = falta.

| # | Funcionalidade | OpenChamber (arquivo:linha) | RustChamber atual (arquivo:linha) | Status | Proposta |
| --- | --- | --- | --- | --- | --- |
| 1 | Top bar com busca, pasta+, display, more | `SidebarHeader.tsx:300-406` | `sidebar.rs:266-310` | P | Manter layout; trocar More e adicionar submenus |
| 2 | Busca de sessões com Enter/Esc/⌘K,L | `SessionSearchInput.tsx:29-98`; `shortcuts/config.ts:131-137` | `sidebar.rs:457-564` | P | Comitar no Enter; atalho `⌘K,L`; busca por ID/branch/projeto/pasta |
| 3 | Display menu completo | `SidebarHeader.tsx:162-235` | `sidebar.rs:339-404` | F | Implementar tabela de 2.3 |
| 4 | Seções Chats/Pinned/In work/Projects | `sessionSidebarRowModel.ts:53-129` | `sidebar.rs:129-208` (só Chats) | F | Criar `SidebarSection` e row model |
| 5 | Caption “Projects” e colapso | `sessionSidebarRowModel.ts:66,1121-1140` | — | F | Adicionar caption e chave de colapso |
| 6 | Agrupamento por projeto | `useSessionGrouping.ts`; `SessionProjectScroller.tsx` | — | F | Agrupar por ownership + `projectID` |
| 7 | Agrupamento por tempo (timeline) | `sessionSidebarRowModel.ts:1200+` | — | F | Lista única ordenada por atividade |
| 8 | Ordenação de projetos (5 modos) | `OC/components/session/sidebar/list/projectSort.ts:22-55` | — | F | Implementar `sortProjectsByOrder` |
| 9 | Exibir todos/um projeto | `SidebarHeader.tsx:203-209` | — | F | Estado + seletor |
| 10 | Linhas detalhadas | `SessionTimelineRowBody.tsx:170-200` | `sidebar.rs:104,815-932` | P | Já existe; persistir e aplicar a works |
| 11 | Nomear worktrees pelo branch | `DOCUMENTATION.md:424-446` | — | F | Só quando worktrees existirem |
| 12 | Recolher/expandir tudo | `useSessionProjectViewState.ts:147-196` | — | F | Implementar 4 conjuntos de colapso |
| 13 | Marcar tudo como lido | não encontrado | — | F | Loop de `POST /view` com watermark otimista |
| 14 | Cabeçalho de projeto com ícone/cor | `sortableItems.tsx:41-111` | `sidebar.rs:566-677` (um workspace) | P | Lista de projetos + ícone/cor |
| 15 | Menu de projeto (nova sessão, Finder, editar, arquivar todas, fechar) | `sortableItems.tsx:208-255` | `sidebar.rs:622-674` (workspace menu) | P | Implementar itens e confirmações |
| 16 | “+” nova sessão em draft | `SessionGroupSection.tsx:701-710` | `sidebar.rs:148-150` chama `create_session` | P | Criar draft antes da sessão |
| 17 | Seletor de projeto com busca e Chat | `DraftTargetSelectors.tsx:388-434,702` | `sidebar.rs:622-674` sem busca | P | Extrair picker reutilizável |
| 18 | Linha de sessão com estados | `SessionNodeItem.tsx:1646-1653`; `SessionTimelineRowBody.tsx` | `sidebar.rs:805-976` | P | Adicionar unread/PR/subagent/terminal |
| 19 | Ações de hover (Track/Done, archive, menu) | `SessionNodeItem.tsx:1870-1960` | `sidebar.rs:962-969` (só menu) | P | Adicionar ações e Shift→delete |
| 20 | Menu de sessão completo | `SessionNodeItem.tsx:1214-1548` | `sidebar.rs:1318-1370` (3 itens) | F | Implementar tabela de 2.6.4 |
| 21 | Renomear inline | `SessionNodeItem.tsx:1228-1242` | `app.rs:747-804` (modal) | P | Inline + Enter/Esc |
| 22 | Renomear com IA | `SessionAiRenameMenuItem.tsx`; `sessionTitle.ts:20-144` | — | F | Ver 7.3; usar OpenCode generate |
| 23 | Copiar ID | `useSessionActions.ts:136-146` | — | F | `arboard` |
| 24 | Fixar / listas de pin | `PinToMenu.tsx`; `sessionPins.ts` | — | F | Metadata OpenCode + lista local |
| 25 | Acompanhar / concluir (In work) | `sessionWorkMetadata.ts`; `session-work/state.js` | — | F | Metadata OpenCode |
| 26 | Exportar Markdown | `exportSession.ts:100-140` | — | F | Reutilizar markdown do chat + filhos |
| 27 | Mover para worktree | `SessionNodeItem.tsx:1294-1397` | — | F | `session.move` + git worktree |
| 28 | Mover sessão / mover para pasta | `MoveSessionDialog.tsx:33-95`; `useSessionFoldersStore.ts` | — | F | `session.move` + store local |
| 29 | Abrir no painel lateral | `SessionNodeItem.tsx:1498-1509` | — | F | Aba no context panel |
| 30 | Mini Chat | `SessionNodeItem.tsx:1511-1522` | — | F | Fora do escopo nativo; avaliar |
| 31 | Arquivar/restaurar/excluir com confirmação | `useSessionActions.ts:148-207` | `sidebar.rs:1339-1356` | P | Confirmação + batch + arquivo |
| 32 | Pastas (criar/renomear/excluir/mover) | `useSessionFoldersStore.ts`; `SessionGroupSection.tsx` | — | F | Store local + DnD egui |
| 33 | Seleção múltipla + Shift range + barra | `useSessionMultiSelectStore.ts`; `BulkActionBar.tsx` | — | F | Estado efêmero + ações em lote |
| 34 | Abas de sessões abertas | `useSessionTabsStore.ts` | — | F | Header tabs + persistência local |
| 35 | Última sessão / por projeto | `last-session-cache.ts`; `SessionSidebar.tsx:61` | `Settings.workspace` apenas | P | `workspace.last_session*` |
| 36 | Drafts de nova sessão | `useSessionDraftsStore.ts` | `drafts.json` por sessão | P | Adicionar drafts por diretório |
| 37 | Worktrees (listar/criar/remover) | `worktreeManager.ts`; `OCS/lib/git/routes.js:1653-1933` | `rchamber-git` sem worktree | F | Adicionar funções git |
| 38 | Adicionar projeto por pasta | `DirectoryExplorerDialog.tsx` | `sidebar.rs:293-298` (rfd) | P | Modal com lista/caminho/status |
| 39 | Clonar repositório | `DirectoryExplorerDialog.tsx:233-713` | — | F | `git clone` via backend |
| 40 | Importar projetos do OpenCode | `OpenCodeProjectImport.tsx` | — | F | `GET /api/project` + dedupe |
| 41 | Status “Adicionado” | `DirectoryExplorerDialog.tsx:982-985` | — | F | Comparar paths normalizados |
| 42 | Fechar projeto sem apagar sessões | `useProjectsStore.ts:792-840` | — | F | Remover do registry local |
| 43 | Arquivar todas as sessões | `SessionProjectScroller.tsx:209-245` | — | F | Filtrar pinned/running/in work |
| 44 | Menu More: Issues/PRs | `SidebarHeader.tsx:246-248` | — | F | Depende de backend de trackers |
| 45 | Menu More: Tarefas agendadas | `SidebarHeader.tsx:250`; `OCS/lib/scheduled-tasks` | — | F | Depende de servidor OpenChamber; ou scheduler local |
| 46 | Menu More: Arquivo | `SidebarHeader.tsx:251` | — | F | Página com `archived=true` |
| 47 | Menu More: Estatísticas | `SidebarHeader.tsx:253-255` | — | F | `GET /api/experimental/session/stats` |
| 48 | Menu More: Catálogo de integrações | `SidebarHeader.tsx:258-260` | — | F | Depende de servidor OpenChamber |
| 49 | Busca no arquivo | `SidebarSearchEmptyState.tsx:16-31` | — | F | Página de arquivo + query |
| 50 | Cache global de sessões + polling 45 s | `useGlobalSessionsStore.ts:74-849` | `backend.rs:820-833` (refresh por evento) | P | Criar store global e paginação |
| 51 | Estrutura incremental pai/filho | `globalSessionStructure.ts` | — | F | `HashMap` no `AppModel` |
| 52 | Ownership root/worktree/space | `sessionOwnership.ts` | — | F | Índice por path |
| 53 | Sessões sem projeto (Chats) | `chatDirectories.ts`; `useDirectoryStore.ts:49-56` | — | F | “Sem projeto” ou raiz `chats` |
| 54 | Persistência de settings por servidor | `persistence.ts`; `registry.ts` | `persist.rs:8-40` | P | `workspace.json` + settings existente |
| 55 | Migrações versionadas | `useSessionDisplayStore.ts:101-149` | `persist.rs` defaults | P | `version` + migrações puras |
| 56 | Atalhos de sessão (⌘K,L/R/Backspace) | `shortcuts/config.ts:90-137` | `app.rs:639-719` | P | Adicionar chords |
| 57 | Foco/teclado em menus | `menu_nav_focus` no RC; OpenChamber usa Radix/Base UI | `sidebar.rs` usa `menu_nav_focus` | P | Manter e expandir |
| 58 | Acessibilidade (aria, focus-visible) | `SidebarHeader.tsx`; `SessionNodeItem.tsx` | egui não expõe ARIA | P | `Response::on_hover_text`, foco por teclado |

---

## 6. Plano de implementação

Sete pacotes independentes, na ordem de dependência. Cada pacote deve terminar com `cargo test` verde e sem comentários novos no código (regra do repositório).

### P0 — Fundação de dados global

**Objetivo:** o app passa a conhecer todas as sessões e todos os projetos, com tipagem de cursor e árvore.

Arquivos-alvo:

- `crates/rchamber-api/src/client.rs`: `list_sessions_page`, `SessionList` tipado com `cursor`, `list_sessions_global`, `list_archived`, `update_session_metadata`, `view_session`, `move_session`, `update_project`.
- `crates/rchamber-api/src/models.rs`: `SessionList`, `SessionCursor`, `SessionPatch`, `ProjectPatch`.
- `crates/rchamber-app/src/action.rs`: `LoadGlobalSessions`, `RefreshProject`, `SetSessionViewed`, `PatchSessionMetadata`, `MoveSession`.
- `crates/rchamber-app/src/backend.rs`: bootstrap global + refresh por diretório + eventos `session.created/renamed/archived/unarchived/viewed/moved/deleted`.
- `crates/rchamber-app/src/state.rs`: `GlobalSessions` (active/archived, `entity_by_id`, `children_by_parent`, `ids_by_directory`, `project_id_by_session`), `ProjectRegistry`.

Dependências: nenhuma.

Riscos:

- `GET /api/session` sem `directory` pode ser caro em bases grandes; paginar com cursor e mostrar a primeira página rápido (modelo OpenChamber, `useGlobalSessionsStore.ts:767-796`).
- OpenCode pode retornar `location` ausente em sessões antigas; preservar o diretório anterior no merge (`mergeSessionDirectoryMetadata`, `globalSessionStructure.ts:50-97`).

Critérios de aceite:

- Teste com 3 diretórios e 2 níveis de `parentID`: a árvore contém todos, sem duplicatas.
- Sessão arquivada aparece em `archived_sessions` e some de `active_sessions`.
- Falha de rede mantém a lista anterior e emite toast; não zera.
- Cursor `next` é seguido até o fim; cursor repetido para.

### P1 — Row model e shell da sidebar

**Objetivo:** substituir a sidebar de “Chats” por um row model com seções, caption, headers, empty states, show more por grupo e busca commitada.

Arquivos-alvo:

- `crates/rchamber-ui/src/sidebar.rs`: dividir em `sidebar/model.rs`, `sidebar/rows.rs`, `sidebar/menus.rs` se necessário (o arquivo tem 1728 linhas; evitar crescer sem estrutura).
- Novo `crates/rchamber-ui/src/sidebar/model.rs`: `SidebarRow`, `SidebarSection`, `build_rows`, `visible_count`, `sticky_headers`.
- `crates/rchamber-ui/src/app.rs`: `UiState` ganha `sidebar_*` persistidos; busca com draft/commit.
- `crates/rchamber-ui/src/persist.rs`: `WorkspaceStore` e migrações.

Dependências: P0.

Riscos:

- Regressão visual da sidebar atual; manter testes existentes (`sidebar.rs:1498-1728`) e adicionar snapshots de altura.
- `egui` não tem virtualização pronta; a lista atual cabe em um `ScrollArea`; se crescer, implementar `show_rows` por altura fixa.

Critérios de aceite:

- Seções Chats/In work/Projects renderizam na ordem e colapsam independentemente.
- Show more por grupo, reset ao colapsar.
- Busca por ID exato, título e path; Esc limpa/fecha; Enter comita.
- Estados vazios e de erro.

### P2 — Projetos: registry, headers, menu, picker, ordenação

**Objetivo:** projetos reais com ícone/cor, agrupamento, menu, seletor, fechar/arquivar.

Arquivos-alvo:

- `crates/rchamber-ui/src/sidebar/projects.rs` (novo): header, menu, picker.
- `crates/rchamber-app/src/state.rs`: `ProjectEntry`, `active_project_id`, `manual_order`.
- `crates/rchamber-ui/src/persist.rs`: persistência de projetos/ordem.
- `crates/rchamber-ui/src/app.rs`: `set_workspace` deixa de ser “um workspace” e passa a “selecionar projeto”, sem reconectar o servidor.

Dependências: P1.

Riscos:

- O backend hoje recria o `Client` ao trocar de workspace (`backend.rs:453-481`), o que dispara reconexão e perde eventos. Projetos devem compartilhar o mesmo servidor; trocar apenas o `directory` do client e o filtro da UI.
- Ordem manual e “recent” podem reordenar sob o ponteiro; usar `holdOrder` (`OC/components/session/sidebar/list/projectSort.ts:94-107`) ou congelar durante hover.

Critérios de aceite:

- Adicionar 3 projetos, reordenar, fechar um; a lista e o ativo persistem após reiniciar.
- Menu de projeto executa nova sessão, revelar no Finder, editar, arquivar todas (com confirmação) e fechar.
- Fechar projeto não apaga sessões.

### P3 — Sessões: linha, estados, menu, rename, pin, work, arquivo

**Objetivo:** paridade funcional do menu de sessão e dos estados visuais.

Arquivos-alvo:

- `crates/rchamber-ui/src/sidebar/session_row.rs` (novo).
- `crates/rchamber-app/src/state.rs`: `is_unread`, `is_pinned`, `work_state`, `viewed`.
- `crates/rchamber-app/src/action.rs`: `ArchiveSession`, `DeleteSession`, `RenameSession`, `SetPinned`, `SetWorkState`, `MarkViewed`, `CopyId`.
- `crates/rchamber-api`: metadata patch, view, archive batch local.
- `crates/rchamber-ui/src/persist.rs`: rename com IA não persiste; pin/work vão para OpenCode.

Dependências: P0, P2 (para o menu de projeto e ownership).

Riscos:

- `PATCH metadata` pode sobrescrever metadata de outro cliente; serializar por sessão e reler.
- Rename inline vs modal: o RustChamber já tem modal; para o screenshot, implementar inline e manter o modal como fallback.
- Excluir com descendentes: coletar a subárvore e confirmar contagem; arquivar descendentes primeiro.

Critérios de aceite:

- Menu de sessão com todos os itens aplicáveis; itens indisponíveis desabilitados com motivo.
- Unread calculado por `idle > viewed`; abrir a sessão chama view e limpa local/remoto.
- Pin e work persistem no OpenCode e aparecem após reload.
- Arquivar remove da lista ativa e coloca no arquivo; restaurar volta.

### P4 — Pastas, seleção múltipla e drafts

**Objetivo:** organização local e operações em lote.

Arquivos-alvo:

- `crates/rchamber-app/src/workspace.rs`: `folders`, `collapsed_folders`, `drafts`.
- `crates/rchamber-ui/src/sidebar/folders.rs` (novo): header, rename inline, menu, DnD básico.
- `crates/rchamber-ui/src/sidebar/selection.rs` (novo): multi-select, range com Shift, barra de ações.
- `crates/rchamber-ui/src/composer.rs`: draft de nova sessão por diretório.

Dependências: P1–P3.

Riscos:

- DnD em egui não é nativo; usar `egui::DragAndDrop` ou um estado `dragging_session` + hit-test de pastas. Não introduzir dependência nova sem necessidade.
- Drafts: o `drafts.json` atual é por `session_id`; um draft de nova sessão não tem ID. Criar `draft_id` e migrar o composer.

Critérios de aceite:

- Criar pasta, arrastar 2 sessões, renomear, excluir com confirmação.
- Selecionar 3 sessões, arquivar em lote, desfazer? (não há undo; confirmar).
- Draft de nova sessão aparece na lista, sobrevive a restart e vira sessão ao enviar.

### P5 — Adicionar projeto, clonar, importar e worktrees

**Objetivo:** fluxos de entrada de projeto e worktree.

Arquivos-alvo:

- `crates/rchamber-ui/src/sidebar/add_project.rs` (novo): modal com rail, path, lista, clone, import.
- `crates/rchamber-git/src/repo.rs`: `clone`, `worktree_list`, `worktree_add`, `worktree_remove`.
- `crates/rchamber-app/src/action.rs`: `CloneRepository`, `AddProject`, `ImportOpenCodeProjects`, `CreateWorktree`, `MoveSessionToWorktree`.
- `crates/rchamber-app/src/backend.rs`: operações de filesystem/git em background.

Dependências: P2, P3.

Riscos:

- Clone é longo; precisa de cancelamento e progresso (não bloquear a UI).
- Importar do OpenCode pode oferecer dezenas de paths; limitar e mostrar contagem.
- Worktree em servidor remoto não é suportado se o git for local; documentar.

Critérios de aceite:

- Adicionar por caminho, por lista e por clone; “Adicionado” correto.
- Importar do OpenCode adiciona só paths disponíveis e não duplica.
- Criar worktree, listar, remover; sessão move para worktree e continua abrindo.

### P6 — Avançados: IA rename, export, seções customizadas, integrações

**Objetivo:** paridade com os itens “extras” do menu e do menu More.

Arquivos-alvo:

- `crates/rchamber-app/src/ai_title.rs` (novo): prompt, coleta de turnos, chamada `POST /api/experimental/generate/text` ou `/api/session/:id/generate`.
- `crates/rchamber-ui/src/export.rs` (novo): Markdown com filhos.
- `crates/rchamber-ui/src/sidebar/sections.rs` (novo): editor de seções/filtros.
- Páginas de Arquivo/Stats; Issues/PRs/Tarefas/Integrações ficam condicionais a um servidor OpenChamber.

Dependências: P3–P5.

Riscos:

- IA rename depende de small model configurado; se não houver, desabilitar com motivo (modelo do OpenChamber, `useSessionAiRenameAction.ts:10-49`).
- Source board/scheduled tasks não existem no OpenCode puro; não fingir que existem. Implementar como “indisponível sem servidor OpenChamber” ou backend próprio.
- Export de transcripts grandes precisa paginar mensagens; não carregar tudo de uma vez.

Critérios de aceite:

- Rename com IA gera título de 2–4 palavras, salva e trata sessão alterada.
- Export gera Markdown com subagentes e escapa conteúdo corretamente.
- Seções customizadas filtram e persistem; limite de seções.

### 6.1 Grafo de dependências

```
P0 ──▶ P1 ──▶ P2 ──▶ P3 ──▶ P4
                    │        │
                    └──▶ P5 ─┘
                              │
                              ▼
                             P6
```

P4 e P5 podem rodar em paralelo depois de P3. P6 é o último.

### 6.2 Estratégia de testes

- **P0**: testes de paginação/cursor, merge de diretório, árvore, arquivamento.
- **P1**: testes de row model (alturas, show more, seções, busca), snapshots de layout.
- **P2**: testes de ordenação, fechar projeto, persistência de ordem.
- **P3**: testes de unread (`idle > viewed`), metadata merge, confirmação de exclusão.
- **P4**: testes de pastas (assign/unassign, colisão de nome, subtree), seleção por range.
- **P5**: testes de clone (mock de `git`), import/dedupe, worktree list.
- **P6**: testes do parser de título, export com filhos, filtros de seção.

Usar o padrão existente: `#[cfg(test)] mod tests` no fim do arquivo, testes puros de funções e `egui::__run_test_ui` para layout (`sidebar.rs:1498-1728`; `app.rs:1556+`).

---

## 7. Endpoints e lacunas

### 7.1 Endpoints do OpenCode usados pelo sidebar

| Endpoint | Uso | Fonte OpenChamber | RustChamber atual |
| --- | --- | --- | --- |
| `GET /api/session` | Lista global/direção, `order`, `limit`, `cursor`, `search`, `parentID`, `archived` | `client.ts:1084-1104`; `globalSessions.ts:79-84` | `list_sessions(100)` com `archived=false` e `parentID=null`; sem cursor (`client.rs:273-299`) |
| `GET /api/session/:id` | Ler registro antes de patch/rename | `client.ts:1106-1111` | `get_session` (`client.rs:301-304`) |
| `POST /api/session` | Criar sessão | `client.ts:1113-1135` | `create_session` (`client.rs:306-334`) |
| `PATCH /api/session/:id` | `title`, `metadata`, `archived` | `session-actions.ts:2039-2069`; `session-archive-batch.ts` | Só `rename_session`/`archive_session`; sem metadata (`client.rs:336-354`) |
| `DELETE /api/session/:id` | Excluir | `session-actions.ts:1599-1648` | `delete_session` (`client.rs:356-358`) |
| `POST /api/session/:id/view` | Marcar lido (`idle`) | `session-unread.ts:47-76` | **falta** |
| `POST /api/session/:id/move` | Mover sessão para diretório | `client.ts:1158-1163`; `MoveSessionDialog.tsx` | **falta** |
| `POST /api/session/:id/fork` | Fork/btw/review | `session-actions.ts` | **falta** (não é essencial no P0) |
| `GET /api/session/:id/message` | Histórico, export, IA rename | `client.ts:1173+` | `messages_page` (`client.rs:364-373`) |
| `GET /api/project` | Importar/validar projetos | `OpenCodeProjectImport.tsx:40-65` | `projects` (`client.rs:556-558`) |
| `PATCH /api/project/:id` | Renomear/ícone canônico (opcional) | — | **falta** |
| `GET /api/event` | SSE de sessões/execução | `client.ts:subscribe` | `subscribe` (`client.rs:618+`) |
| `GET /api/experimental/session/stats` | Estatísticas | `sidebar` More | **falta** |
| `POST /api/experimental/generate/text` | Rename com IA | `small-model/client.js` | **falta** |
| `POST /api/session/:id/generate` | Alternativa de IA no modelo da sessão | `small-model/DOCUMENTATION.md` | **falta** |

Observação: o `GET /api/session` do OpenCode aceita `archived` ausente para incluir ambos (`OP/packages/schema/src/session.ts:90-101`). O RustChamber deve omitir `archived` e dividir no cliente, como o OpenChamber, porque o filtro do servidor não vê o arquivo legado do OpenChamber — e para o RustChamber puro, omitir é o mais simples.

### 7.2 Endpoints do servidor OpenChamber (não existem no OpenCode puro)

| Endpoint | Uso | Fonte |
| --- | --- | --- |
| `GET/PUT /api/config/settings` | Settings (projetos, display, seções) | `OCS/lib/opencode/routes.js:97-301` |
| `GET/POST/PATCH /api/session-folders` | Pastas | `OCS/lib/session-folders/routes.js:187-255` |
| `GET/PUT/DELETE /api/session-drafts/:id` | Drafts | `OCS/lib/session-drafts/routes.js:158-239` |
| `POST /api/projects/session-directory` | Registrar diretório de sessão | `OCS/lib/projects/session-directory.js:31` |
| `PUT/DELETE /api/projects/:id/icon`, `POST /icon/discover` | Ícones | `useProjectsStore.ts:984-1077` |
| `POST /api/openchamber/sessions/archive` / `unarchive` | Arquivamento em lote + fallback | `OCS/lib/openchamber-sessions/routes.js:1600-1652` |
| `GET/POST /api/openchamber/sessions/:id/metadata` | Merge de metadata | `OCS/lib/openchamber-sessions/routes.js:1621-1643` |
| `GET/PUT /api/session-warming` | Aquecimento por sessão | `OC/stores/useSessionWarmingStore.ts:1-131` |
| `POST /api/fs/clone` / `/api/fs/reveal` | Clonar/revelar | `OCS/lib/fs/routes.js:875,1812` |
| `GET/POST/DELETE /api/git/worktrees` | Worktrees | `OCS/lib/git/routes.js:1653-1933` |
| `GET/POST /api/projects/:id/scheduled-tasks` | Tarefas agendadas | `OCS/lib/scheduled-tasks/routes.js:18-159` |
| `/api/small-model` / `generate` | IA de título | `OCS/lib/small-model/routes.js` |
| `/api/sessions/:id/view` | Badge/push view (complementar) | `OC/sync/notification-store.ts:58` |

Para o RustChamber, esses endpoints só existem se o usuário estiver conectado a um servidor OpenChamber. O caminho recomendado é **não depender deles**: implementar pastas/drafts/settings localmente e usar o OpenCode para pins/work/archive/view/move.

### 7.3 Lacunas na `rchamber-api`

| Lacuna | Impacto | Proposta de assinatura |
| --- | --- | --- |
| Lista global/cursor | Não vê todos os projetos; não vê subsessões; não vê arquivadas | `list_sessions_page(&self, opts: SessionListOptions) -> Result<SessionPage>` com `global: bool`, `archived: Option<bool>`, `parent_id: Option<Option<&str>>`, `cursor`, `search`, `order`, `limit` |
| `SessionList` tipado | `cursor` é `Value` | `struct SessionCursor { previous: Option<String>, next: Option<String> }` |
| `PATCH` metadata | Pins/work não persistem | `update_session(&self, id, patch: SessionPatch) -> Result<()>`; `SessionPatch { title, metadata, archived }` |
| `view` | Unread nunca limpa | `view_session(&self, id, idle: f64) -> Result<()>` |
| `move` | Não move sessão | `move_session(&self, id, directory: &str) -> Result<()>` |
| `project update` | Renomear projeto no OpenCode | `update_project(&self, id, patch: ProjectPatch) -> Result<Project>` |
| `generate` | IA rename | `generate_text(&self, prompt, system, directory, preferred_provider, preferred_model) -> Result<String>` |
| `stats` | Página de estatísticas | `session_stats(&self, from, to, project, timezone) -> Result<Value>` |
| `fork` | Review/btw | `fork_session(&self, id, message_id: Option<&str>) -> Result<Session>` |
| `export/import` | Export Markdown/transferência | Opcional; o export Markdown pode ser montado pelo cliente com mensagens |
| Batch archive | Arquivar muitas sessões | OpenCode não tem; loop no cliente ou rota própria do RustChamber |

### 7.4 Eventos que o backend precisa tratar

Hoje `live.rs` trata `session.created/moved/archived/unarchived/forked/deleted` como `SessionsDirty` (`RC/rchamber-app/src/live.rs:20-32`). Para a sidebar global, isso é suficiente para refresh, mas não para atualização incremental. Adicionar:

- `session.renamed` / `session.viewed` → upsert local, sem refresh completo (`session.updated` existe só no contrato v1).
- `session.moved` → mudar `directory` e reindexar.
- `session.deleted` → remover do global e fechar abas.
- `session.archived` / `unarchived` → mover entre listas.
- `project.updated` → atualizar registry.

O OpenChamber faz isso no `event-reducer.ts`/`session-event-router.ts`; o RustChamber pode começar com `SessionsDirty` + debounce 180 ms (já existe, `backend.rs:760-818`), e só depois incremental.

---

## 8. Layout, tokens, ícones e acessibilidade

### 8.1 Medidas

| Elemento | OpenChamber | RustChamber atual | Ajuste |
| --- | --- | --- | --- |
| Largura da sidebar | 280 padrão; 264–500 | 280 padrão; 264–500 | igual (`Sidebar.tsx:13-14`; `app.rs:33-34`) |
| Top bar | 40 px + busca 57 px | 40 + 57 | igual (`sidebar.rs:15,267`) |
| Botão de ação | 24 px, ícone 16 | 24/16 | igual (`sidebar.rs:20-21`) |
| Linha de sessão 1 linha | 32 px (`py-[5px]`) | 34 px | reduzir 2 px ou manter; o OpenChamber é mais denso |
| Linha detalhada | 52 px (20 + 16 + padding) | 52 px | igual (`ROW_HEIGHT_DETAILED`) |
| Header de seção | 34 px | 28 px | aumentar para 34 |
| Header de projeto | ~32 px | 32 px | igual |
| Show more | 28 px | 28 px | igual |
| Caption | 34 px | — | adicionar |
| Recuo de árvore | 12 px/nível, base 6 px | 16 px/nível | reduzir para 12 |
| Texto da linha | 13.5 px | 13.5 px | igual |
| Meta | 12 px | 12.5 px | padronizar 12 |
| Título de seção | 15 px | 15 px | igual |
| Ícone de projeto | 14 px | 14 px | igual |
| Ícone de estado | 12 px | 12 px | igual |
| Raio de linha | 7 px (`radius-md`) | 7 px | igual |
| Raio de popup | 9 px (`radius-lg`) / 12 px | 12 px | ver `theme.rs:1068` |
| Padding do menu | 8 px | 12 px (frame) | alinhar a 8–10 px |

### 8.2 Tipografia

- `typography-ui-section` = 0.9375 rem (15 px) para labels de projeto e caption.
- `typography-ui-label` = 0.84375 rem (13.5 px) para linhas, menus e botões.
- `typography-meta` = 0.78125 rem (12 px) para meta e dicas.
- `typography-micro` = 0.75 rem (12 px) para tempo, contagens e rodapé.
- Peso: projeto `font-medium`; título de sessão `normal`; não lido/pendente `semibold`; selecionado mantém peso e troca cor.
- Fonte: Inter (UI), JetBrains Mono (paths/código). O RustChamber já empacota Inter/JetBrains Mono (`theme.rs:14-23`).

### 8.3 Cores e estados

- `oc-title-rest`: `color-mix(in oklab, var(--foreground) 50%, var(--muted-foreground))` (`OC/index.css:210-216`).
- `oc-group-label`: 75% foreground + 25% muted (`OC/index.css:218-223`).
- Seleção: `--interactive-selection` com foreground; hover: `--interactive-hover` com 60% de opacidade na linha.
- Estados: info (azul), success (verde), warning (amarelo), destructive (vermelho). O RustChamber já tem `p.blue`, `p.green`, `p.coral`, `p.red`; mapear para tokens semânticos, não para nomes de cor.
- Fundo da sidebar: `--sidebar` (um passo do `background`); borda direita 0.5 px (`sidebar.rs:217-221`).

### 8.4 Ícones

Mapa mínimo (nomes do OpenChamber → `Icon` do RustChamber):

| Ação | OpenChamber | RustChamber |
| --- | --- | --- |
| Busca | `search` | `Search` |
| Adicionar projeto | `folder-add` | `Folder` (adicionar `FolderPlus` se necessário) |
| Display | `equalizer-2` | `Menu` (trocar para sliders) |
| More | `more` | `More` |
| Nova sessão | `add` | `Plus` |
| Renomear | `pencil-ai` | `Pencil` |
| Copiar | `file-copy` | adicionar `Copy` |
| Pin | `pushpin`/`pin` | adicionar `Pin` |
| Arquivar | `inbox-archive`/`archive` | `Archive` |
| Excluir | `delete-bin` | `Trash` |
| Worktree | `git-branch`/`folder-shared` | `Branch`/`Folder` |
| Pasta | `folder` | `Folder` |
| Mover | `folder-received` | adicionar `FolderInput` |
| Painel lateral | `chat-4` | `PanelRight` (se existir) |
| Mini Chat | `window` | `Window` |
| Issues/PRs | `git-pull-request` | adicionar `PullRequest` |
| Tarefas | `calendar-schedule` | `Calendar` |
| Estatísticas | `bar-chart` | `Chart` |
| Integrações | `store-2` | `Store` |
| Check | `check` | `Check` |
| Chevron | `arrow-right-s`/`arrow-down-s` | `ChevronRight`/`ChevronDown` |
| Permissão | `shield` | `Shield` |
| Pergunta | `question` | `Question` |
| Rodando | `circle` (spinner opcional) | anel atual |
| Não lido | `checkbox-circle` | `Check` (trocar) |
| Pin muted | `pushpin` | `Pin` muted |

O RustChamber tem `icons.rs` com um enum pequeno; adicionar ícones é barato, mas manter o conjunto mínimo e consistente.

### 8.5 Menus e popovers

- Largura mínima 180–200 px; item com ícone 16 px + label 13.5 px + atalho à direita em 12 px muted.
- Separador de 1 px com 4 px de margem; grupos de escolha com check à direita.
- Submenu abre à direita; item atual desabilitado ou com check.
- Somente um menu aberto por vez: o OpenChamber usa `openSidebarMenuKey` compartilhado (`useSessionRowMenuState.ts`); o RustChamber já fecha menus no Esc e no clique fora (`app.rs:700-719`).
- O menu de sessão deve abrir alinhado ao fim da linha, com largura 180 px; o de projeto, alinhado ao início, 200–250 px.

### 8.6 Movimento

- Popup: 251 ms com easing de mola; saída 110 ms (`OC/styles/design-system.css:100-104`).
- Layout: 294 ms (`105-106`).
- Disclosure de linha: 200 ms (`sidebar.rs:46`; `DOCUMENTATION.md:242-259`).
- Reduzir movimento: `reduce_motion` desliga animações; o RustChamber já tem `motion::set_reduced_motion` e `motion_time` (`RC/rchamber-ui/src/app.rs`; `motion.rs`).
- Hover de ícones: 120–150 ms; não animar a linha inteira a cada movimento.

### 8.7 Acessibilidade e teclado

- Todo controle tem `aria-label`/tooltip; o RustChamber usa `on_hover_text`.
- Foco visível: anel de 2 px no tema; `Response::highlight()` ou `WidgetVisuals`.
- Menus: setas navegam, Enter ativa, Esc fecha, Home/End opcional.
- Linha: Enter/Espaço abre; Shift+clique range; Ctrl/Cmd+clique abre em painel lateral (OpenChamber não usa Ctrl para seleção; usa Shift e o modo de seleção).
- Botões de hover devem aparecer também com foco de teclado; em egui, manter as ações sempre visíveis quando a linha tem foco.
- Não depender só de cor: ícones distintos para cada estado.

### 8.8 Checklist de fidelidade ao screenshot

- [ ] Top bar: lupa, pasta+, display, “...” com 24 px e ícones 16 px.
- [ ] Seção “Conversas” com label muted 15 px e chevron.
- [ ] “Em andamento” como sublabel, com sessões em uma linha e spinner/anel azul.
- [ ] “Projetos” como caption + headers com ícone colorido de 14 px.
- [ ] Projeto com divisor sutil antes (12 px) e sem linha entre projetos colapsados.
- [ ] Sessão ativa com fill de seleção e texto de seleção.
- [ ] Check verde em sessão concluída/não lida, não em todas.
- [ ] “Mostrar mais” alinhado ao texto, 12 px muted.
- [ ] Menu de display com submenus e checks.
- [ ] Menu de projeto com “Fechar projeto” em vermelho.
- [ ] Menu de sessão com “Excluir” em vermelho e submenus de pin/worktree/pasta.
- [ ] Modal de adicionar projeto com rail, path mono, lista, badge “Adicionado”.
- [ ] Modal de clone com URL, destino, lista e “Clonar e adicionar”.
- [ ] Dropdown de projeto com busca e check.

---

## Apêndice A — Mapa de arquivos

OpenChamber UI:

- `OC/components/session/SessionSidebar.tsx` — composição da sidebar, busca, diálogos, worktrees, active session por projeto.
- `OC/components/session/sidebar/shell/SidebarHeader.tsx` — top bar, display menu, more menu, busca.
- `OC/components/session/sidebar/shell/SidebarFooter.tsx` — settings, update, instância.
- `OC/components/session/sidebar/sessionSidebarRowModel.ts` — row model.
- `OC/components/session/sidebar/SessionSidebarRows.tsx` — renderizador virtual.
- `OC/components/session/sidebar/projects/*` — headers, menu, picker, agrupamento, sort, worktree.
- `OC/components/session/sidebar/sessions/*` — linha, menu, ações, status, árvore.
- `OC/components/session/sidebar/folders/*` — pastas, DnD, bulk actions.
- `OC/components/session/sidebar/sections/*` — seções, Pin to, manager.
- `OC/components/session/sidebar/works/*` — worktree/espaço, drafts.
- `OC/components/session/sidebar/list/*` — coleta global, sync, ownership, prefetch.
- `OC/components/session/DirectoryExplorerDialog.tsx` — adicionar/clonar projeto.
- `OC/components/session/MoveSessionDialog.tsx` — mover sessão.
- `OC/components/session/SessionSearchInput.tsx` — input de busca.
- `OC/components/session/project-import/OpenCodeProjectImport.tsx` — importar do OpenCode.

OpenChamber stores/sync:

- `OC/stores/useProjectsStore.ts`
- `OC/stores/useGlobalSessionsStore.ts`
- `OC/stores/globalSessionStructure.ts`
- `OC/stores/globalSessions.ts`
- `OC/stores/useSessionDisplayStore.ts`
- `OC/stores/useSessionFoldersStore.ts`
- `OC/stores/useSessionPinnedStore.ts`
- `OC/stores/useSessionTabsStore.ts`
- `OC/stores/useSessionDraftsStore.ts`
- `OC/stores/useSessionMultiSelectStore.ts`
- `OC/stores/useUIStore.ts`
- `OC/stores/useProjectContextStore.ts`
- `OC/stores/useSessionWarmingStore.ts`
- `OC/stores/useDirectoryStore.ts`
- `OC/sync/session-actions.ts`
- `OC/sync/session-unread.ts`
- `OC/sync/session-pins.ts`
- `OC/sync/session-worktree-store.ts`
- `OC/sync/persist-cache.ts`
- `OC/sync/worktree-topology-cache.ts`
- `OC/sync/last-session-cache.ts`
- `OC/sync/session-activity-timing.ts`

OpenChamber server:

- `OCS/index.js` — data dir, settings path, registro de rotas.
- `OCS/lib/opencode/routes.js` — settings/config.
- `OCS/lib/session-folders/routes.js`
- `OCS/lib/session-drafts/routes.js`
- `OCS/lib/openchamber-sessions/*`
- `OCS/lib/projects/*`
- `OCS/lib/git/routes.js`
- `OCS/lib/fs/routes.js`
- `OCS/lib/scheduled-tasks/*`
- `OCS/lib/session-work/*`
- `OCS/lib/small-model/*`

OpenCode:

- `OP/packages/schema/src/session.ts` — contrato de sessão/lista.
- `OP/packages/protocol/src/groups/session.ts` — endpoints.
- `OP/packages/schema/src/project.ts` — projeto.
- `OP/packages/util/src/global-roots.ts` — paths.
- `OP/packages/cli/src/database-path.ts` — DB.
- `OP/packages/core/src/session/store.ts` — store de sessões.

RustChamber:

- `RC/rchamber-ui/src/sidebar.rs` — UI atual.
- `RC/rchamber-ui/src/app.rs` — `UiState`, `RustChamber`, layouts, rename modal, teclado.
- `RC/rchamber-ui/src/persist.rs` — settings e drafts.
- `RC/rchamber-ui/src/theme.rs` — tokens.
- `RC/rchamber-ui/src/palette.rs` — command palette.
- `RC/rchamber-app/src/state.rs` — `AppModel`, `Chat`.
- `RC/rchamber-app/src/action.rs` — `Action`, `BackendEvent`.
- `RC/rchamber-app/src/backend.rs` — ator, bootstrap, refresh, eventos.
- `RC/rchamber-app/src/live.rs` — tradução de eventos.
- `RC/rchamber-api/src/client.rs` — HTTP.
- `RC/rchamber-api/src/models.rs` — modelos.
- `RC/rchamber-git/src/repo.rs` — git.

## Apêndice B — Riscos e decisões em aberto

1. **Servidor único vs múltiplos diretórios.** O OpenCode atual é um servidor global; trocar de projeto não deve recriar o `Client` nem reiniciar o servidor. O RustChamber hoje recria (`backend.rs:453-481`). Isso precisa mudar no P0/P2.
2. **Metadata do OpenCode.** `PATCH` aceita metadata, mas o OpenChamber usa uma rota própria de merge porque precisa de merge patch e fallback. O RustChamber deve testar o comportamento do servidor alvo; se `PATCH` substituir o objeto inteiro, fazer read-modify-write e serializar por sessão.
3. **Sessões sem projeto.** O OpenChamber tem “Chats” com raiz gerenciada; o RustChamber não tem. Decidir entre “Sem projeto” (lista simples) ou uma raiz `chats` de verdade. Para a primeira versão, “Sem projeto” é mais honesto e menos código.
4. **Worktrees em servidor remoto.** `rchamber-git` roda local; se o endpoint for remoto, worktrees não funcionam. Documentar como limitação ou mover a descoberta para o servidor.
5. **Tarefas agendadas / Issues e PRs / Integrações.** São features do servidor OpenChamber, não do OpenCode. Não prometer paridade sem backend. Opções: (a) esconder os itens quando não há servidor OpenChamber; (b) implementar scheduler local e trackers próprios; (c) aceitar um servidor OpenChamber opcional.
6. **Mark all as read.** Não existe no código atual. Implementar como ação local em lote; não inventar endpoint. Se o servidor OpenChamber estiver presente, poderia ganhar uma rota em lote, mas o OpenCode puro não tem.
7. **Título com IA.** O prompt do OpenChamber é específico (2–4 palavras, sem paths, mesmo idioma). Reutilizar o texto e o parser; não usar o título nativo do OpenCode sem o plugin, pois ele gera títulos longos.
8. **Performance.** O row model do OpenChamber é altamente otimizado (memoização, virtualização, estruturas incrementais). O RustChamber não precisa copiar a complexidade toda no P1, mas deve evitar reconstruir a árvore a cada frame: calcular no máximo uma vez por mudança de estado e guardar em `AppModel`/`UiState`.

---

Fim do estudo.
