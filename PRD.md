# App Manager — Product Requirements Document

- **Versão:** 0.1 (rascunho)
- **Data:** 2026-10-27
- **Estado:** em discussão — decisões abertas na secção 11
- **Plataforma alvo:** Pop!_OS / COSMIC (apt + dpkg), Flatpak, AppImage
- **Licença:** GPL-3.0 (proposta, alinhada com o resto dos projetos)

---

## 1. Problema

Os gestores de apps no Linux são confusos e limitados:

- Misturam apps do sistema com apps instaladas pelo utilizador, sem distinção útil.
- Desinstalar não limpa nada: fica configuração, cache, dados, entradas de menu, ícones, unidades systemd.
- Mostram dependências e pacotes técnicos (Synaptic) a quem só quer remover uma app.
- Não há forma de instalar nem atualizar apps que vivem no GitHub, fora dos repositórios da distro, Flatpak e Snap. Atualizar essas apps é manual, por app, sempre.

## 2. Visão

Uma app única, leve e nativa, onde o utilizador vê **apps** e não pacotes, com duas metades:

1. **Remover sem drama** — desinstala qualquer app sem partir o sistema, sem mostrar dependências, e limpa o que é comprovadamente dela. Inspiração: Revo Uninstaller.
2. **Instalar e atualizar do GitHub** — cola-se o URL do repo, a app resolve o release certo, instala, e diz quando há versão nova (mesmo que o check seja manual).

## 3. Princípios (não negociáveis)

| # | Princípio | Consequência |
|---|-----------|--------------|
| P1 | **Segurança acima de completude** | Se há dúvida, o pacote fica. Nunca se remove algo partilhado. |
| P2 | **O utilizador vê apps, não pacotes** | A interface fala de aplicações. Pacotes e dependências são detalhe interno. |
| P3 | **Nada de remoções silenciosas** | Antes de agir, mostra-se o que vai sair e o que fica, e porquê. |
| P4 | **Nada de privilégios escondidos** | Toda a ação com root é precedida de uma explicação clara do que vai correr. |
| P5 | **Leve e rápido** | Nativo, sem runtime web, sem daemon desnecessário. |

## 4. Público-alvo

- **Primário:** utilizadores de Linux com perfil técnico-médio (sabem o que é um `.deb`, não querem usar o terminal para gerir apps).
- **Contexto de uso:** desktop COSMIC/Pop!_OS, uma app por vez, sessão gráfica.
- **Não-alvo:** administradores de servidores, ambientes sem desktop, utilizadores que querem controlo total de versões e pinning.

## 5. Âmbito

### 5.1 Dentro

- Inventário de apps instaladas via apt/dpkg, Flatpak e AppImage.
- Desinstalação segura, com limpeza de vestígios.
- Instalação a partir de um URL de repo GitHub (`.deb` e AppImage).
- Verificação e aplicação de atualizações para apps geridas pela app.

### 5.2 Fora (non-goals da v1)

- Navegar/browse repositórios ou pesquisar pacotes (não é um Synaptic).
- Compilar a partir de código-fonte.
- "System cleaner" genérico que apaga ficheiros à procura de espaço.
- Loja com reviews, ratings, curadoria de catálogo.
- Daemon de atualização automática em background.
- Suporte multi-distro (Fedora/Arch) na v1 — arquitetura preparada, mas só apt implementado.
- Snap (a confirmar; não é prioritário).

### 5.3 Anti-goals (falha mesmo que funcione)

- Deixar o sistema num estado que não arranca ou com pacotes quebrados.
- Apagar dados do utilizador sem aviso explícito.
- Reportar sucesso numa instalação/remoção que falhou.
- Ser mais lenta ou mais pesada que o GNOME Software.
- Esconder o que faz quando pede root.

## 6. Requisitos funcionais

Prioridade: **M** = obrigatório na v1 · **S** = desejável · **C** = futuro.

### 6.1 Inventário

| ID | Requisito | Prio |
|----|-----------|------|
| FR-01 | Listar apps instaladas via dpkg, Flatpak e AppImage. | M |
| FR-02 | Separar "instaladas por mim" de "do sistema", com base em `apt-mark showmanual`, prioridade/essential do dpkg, origem Flatpak e presença de AppImage. | M |
| FR-03 | Mostrar por app: nome, ícone, versão, origem, tamanho. | M |
| FR-04 | Identificar apps geridas por esta app (instaladas do GitHub). | M |
| FR-05 | Atualizar o inventário a pedido, sem bloquear a interface. | M |
| FR-06 | Assinalar apps que não são seguras de remover, sem as esconder. | S |

### 6.2 Remoção

| ID | Requisito | Prio |
|----|-----------|------|
| FR-10 | Remover uma app com uma ação, sem terminal. | M |
| FR-11 | Calcular o conjunto de remoção com a simulação do próprio apt (`apt-get -s`), de modo a nunca divergir do que o apt faria. | M |
| FR-12 | Nunca remover pacotes essential/required, nem nada de que dependa uma app que fica. | M |
| FR-13 | Mostrar pré-visualização: o que sai, o que fica, e a razão de ficar. | M |
| FR-14 | Oferecer purge da configuração do sistema vs. manter. | S |
| FR-15 | Detetar vestígios: `~/.config`, `~/.local/share`, `~/.cache`, entradas `.desktop`, ícones, unidades systemd de utilizador. | S |
| FR-16 | Mover vestígios para o lixo em vez de apagar, com opção explícita de apagar de vez. | S |
| FR-17 | Flatpak: desinstalar com opção de apagar dados (`--delete-data`). | M |
| FR-18 | AppImage: remover o ficheiro, a entrada de menu e o ícone associados. | M |
| FR-19 | Registar cada remoção num histórico consultável. | S |
| FR-20 | Recusar e explicar quando a remoção é insegura, em vez de a executar parcialmente. | M |

### 6.3 Instalação do GitHub

| ID | Requisito | Prio |
|----|-----------|------|
| FR-30 | Aceitar um URL de repo e resolver o release mais recente via API do GitHub. | M |
| FR-31 | Escolher o asset correto por arquitetura e formato (`.deb` preferido sobre AppImage). | M |
| FR-32 | Mostrar origem, versão, tamanho e o que vai ser executado antes de pedir privilégios. | M |
| FR-33 | Instalar `.deb` via apt, com resolução de dependências. | M |
| FR-34 | Instalar AppImage em diretório do utilizador, com integração no menu e ícone. | M |
| FR-35 | Guardar o vínculo app ↔ repo ↔ versão instalada. | M |
| FR-36 | Avisar quando o asset não é reconhecível, em vez de adivinhar. | M |

### 6.4 Atualizações

| ID | Requisito | Prio |
|----|-----------|------|
| FR-40 | Verificar atualizações por app e para todas, por ação manual. | M |
| FR-41 | Comparar a versão upstream com a instalada, tolerando esquemas de versão diferentes. | M |
| FR-42 | Aplicar a atualização pelo mesmo fluxo da instalação, preservando dados do utilizador. | M |
| FR-43 | Respeitar os limites de pedidos da API do GitHub (cache de resultados). | S |
| FR-44 | Mostrar link para as release notes. | C |

## 7. Requisitos não-funcionais

| ID | Requisito |
|----|-----------|
| NFR-01 | Arranque a quente abaixo de 300 ms até à lista utilizável. |
| NFR-02 | Nenhuma operação bloqueia a interface; progresso sempre visível. |
| NFR-03 | Consumo em repouso abaixo de 100 MB. |
| NFR-04 | Funciona offline para operações locais; falha de rede é erro explícito, não silêncio. |
| NFR-05 | Toda a ação privilegiada é descrita antes de ser executada. |
| NFR-06 | Rust + libcosmic; sem runtime web. |
| NFR-07 | Textos internacionalizáveis desde o início (Fluent). |
| NFR-08 | Sem telemetria. |

## 8. Modelo de segurança

- **Nunca** `curl | sh`, nem execução de `install.sh` de terceiros na v1.
- Downloads só por HTTPS; se o release publicar checksums, verificar.
- `.deb` do GitHub é código não assinado: aviso explícito e visível, não um rodapé.
- Ações com root via helper pequeno invocado por polkit/pkexec, com descrição do propósito no pedido de password.
- O plano de remoção é calculado antes da execução e não muda a meio.

## 9. Modelo de dados (conceptual)

`App`: id, nome, ícone, versão, origem (`apt` | `flatpak` | `appimage` | `github`), tamanho, caminho(s), pacote(s) associado(s), repo GitHub e método de instalação (quando aplicável), data de instalação, se é gerida pela app.

`RemovalPlan`: app, pacotes a remover, pacotes que ficam e porquê, ficheiros/vestígios, tamanho total, se é seguro.

`UpdateCheck`: app, versão instalada, versão upstream, asset candidato, resultado (`atualizada` | `desatualizada` | `desconhecida`).

## 10. Métricas de sucesso

- Remover uma app de ponta a ponta em menos de 30 s, sem terminal.
- Zero sistemas quebrados na matriz de testes (essential, dependência partilhada, Flatpak, AppImage).
- Atualizar uma app do GitHub em 2 cliques ou menos.
- Resolver corretamente pelo menos 90% dos layouts de release habituais (deb único, AppImage único, múltiplos assets por arquitetura).

## 11. Decisões abertas

| # | Decisão | Opções | Recomendação |
|---|---------|--------|--------------|
| D1 | Que metade primeiro | Removedor ou instalador | **Inventário + removedor** (a base serve os dois, e é o que não existe em lado nenhum) |
| D2 | Limpeza de vestígios | Apagar vs. mover para o lixo | Lixo por omissão, apagar só com confirmação |
| D3 | Privilégios | `pkexec` por ação vs. helper persistente | Helper pequeno, sem daemon |
| D4 | Flatpak | Só gerir vs. instalar também | Só gerir na v1 |
| D5 | Nome do produto | "App Manager" é provisório | A decidir antes da fase 4 |
| D6 | Distros | Só apt vs. multi-distro | Só apt na v1, backend abstrato |
| D7 | Apps de sistema na interface | Tudo numa lista vs. separar protegidas | **Separar em "Protegidas" e "Removíveis"**; protegidas sem ação de remover |

## 12. Riscos

| Risco | Impacto | Mitigação |
|-------|---------|-----------|
| Heurística de "app" vs "pacote" falha | Lista confusa ou perigosa | Mapear por `.desktop` + `dpkg -S`, não por nome |
| Assets do GitHub inconsistentes | Instalação falha ou instala o errado | Regras explícitas + recusa em vez de adivinhação |
| Limpeza de vestígios apanha ficheiros de outra app | Perda de dados | Só diretórios com correspondência forte; resto é sugestão, não ação |
| API do GitHub sem autenticação | Limite de 60 pedidos/hora | Cache + verificação manual por app |

---

*Este documento descreve o quê e o porquê. O como e o quando estão em `ROADMAP.md`.*
