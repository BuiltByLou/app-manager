# App Manager — Implementation Roadmap

- **Versão:** 0.1 (rascunho)
- **Data:** 2026-10-27
- **Estado:** plano proposto, depende da decisão D1 (ver `PRD.md`)
- **Regra:** cada fase termina com uma definição de pronto verificável. Não se avança com pendências silenciosas.

---

## Como usar

- Uma fase por pull request grande ou conjunto pequeno de PRs.
- Ao fechar uma fase, atualizar a tabela **Estado** e o **Registo de alterações** no fim deste documento.
- Desvios ao plano são anotados, não corrigidos em silêncio.

## Estado

| Fase | Nome | Estado |
|------|------|--------|
| 0 | Fundação: inventário read-only | Concluída (inventário em 266 ms, interface em inglês) |
| 1 | Remoção segura | Em curso (pré-visualização real com `apt-get -s` e execução escrita; falta confirmar num desktop com agente polkit) |
| 2 | Instalação a partir do GitHub | Por começar |
| 3 | Atualizações | Por começar |
| 4 | Publicação | Por começar |

---

## Fase 0 — Fundação: inventário read-only

**Objetivo:** saber o que está instalado e apresentá-lo bem, sem escrever nada no sistema.

**Entregas**
- Crate de backend com um leitor por fonte: dpkg/apt, Flatpak, AppImage.
- Modelo `App` unificado (ver PRD §9).
- Classificação "instalado por mim" vs "sistema":
  - apt: `apt-mark showmanual` + prioridade/essential do dpkg;
  - Flatpak: origem e instalação do utilizador;
  - AppImage: descoberta em `~/Applications`, `~/.local/bin`, `~/bin`, `/opt` e mapeamento por `.desktop`.
- Mapeamento app ↔ pacote(s) via entradas `.desktop` e `dpkg -S` (nunca por semelhança de nome).
- Interface libcosmic: lista, ícones, versão, origem, tamanho, pesquisa e refresh manual.
- Testes unitários sobre fixtures de saída real (`dpkg-query`, `flatpak list`).

**Definição de pronto**
- Numa máquina Pop!_OS real, a lista corresponde ao que o sistema tem, sem duplicados nem fantasmas.
- Nenhuma operação de escrita; a app pode ser usada em modo leitura.
- Arranque a quente dentro do alvo NFR-01.

**Riscos:** apps sem `.desktop` (CLI, snaps) e AppImages sem integração. Devem aparecer numa secção "sem metadados", não desaparecer.

---

## Fase 1 — Remoção segura

**Objetivo:** remover apps sem terminal e sem partir nada.

**Entregas**
- `RemovalPlan` calculado com a simulação do apt (`apt-get -s remove/purge`), usando o apt como fonte de verdade.
- Bloqueio de essential/required e de qualquer pacote com dependentes instalados.
- Pré-visualização: o que sai, o que fica, porquê. Sem jargão de dependências.
- Execução via helper privilegiado (polkit/pkexec), com descrição do propósito.
- Flatpak: `flatpak uninstall` e opção `--delete-data`.
- AppImage: remoção do ficheiro, `.desktop` e ícone.
- Deteção de vestígios em `~/.config`, `~/.local/share`, `~/.cache`, unidades systemd de utilizador; envio para o lixo por omissão.
- Histórico de remoções.
- Matriz de testes: pacote essential, dependência partilhada, app Flatpak com dados, AppImage integrado.

**Definição de pronto**
- A matriz de testes passa sem nenhum sistema quebrado.
- Uma remoção partilhada mostra a biblioteca a ficar e explica porquê.
- Cancelar a password não deixa o sistema num estado intermédio.

**Riscos:** ficheiros de configuração partilhados entre apps (ex.: `~/.config/gtk-4.0`). Regra: só se apaga o que tem o nome ou o id da app e nenhum outro consumidor conhecido.

---

## Fase 2 — Instalação a partir do GitHub

**Objetivo:** colar um URL e ter a app instalada.

**Entregas**
- Resolver: `releases/latest` da API do GitHub, escolha de asset por arquitetura e formato.
- Regras explícitas de formato: `.deb` preferido, AppImage como alternativa, resto recusado com mensagem clara.
- Diálogo de confirmação com origem, versão, tamanho, checksum (se existir) e o comando que vai correr.
- Instalação de `.deb` via `apt install ./ficheiro.deb`.
- Instalação de AppImage em `~/Applications`, `chmod +x`, extração de ícone e criação de `.desktop`.
- Persistência do vínculo app ↔ repo ↔ asset ↔ versão.
- Cache de metadados do GitHub e tratamento do limite de pedidos.

**Definição de pronto**
- Instalar com sucesso três apps reais do utilizador (uma `.deb`, uma AppImage, uma com múltiplos assets).
- Repo sem asset compatível produz um erro compreensível, não uma instalação errada.
- A app instalada aparece no inventário já ligada ao repo.

**Riscos:** repos que só publicam source, assets com nomes opacos, releases em draft/prerelease. Todos devem cair em "não suportado", nunca em tentativa às cegas.

---

## Fase 3 — Atualizações

**Objetivo:** saber o que está desatualizado e atualizar.

**Entregas**
- Verificação manual por app e "verificar todas".
- Comparação de versões com tolerância a esquemas diferentes (`v1.2.3`, `1.2.3-beta`, datas).
- Atualização pelo mesmo fluxo da fase 2, preservando dados.
- Estado explícito `desconhecida` quando não é possível comparar — nunca falso "atualizada".
- Cache e limites de API.

**Definição de pronto**
- Uma `.deb` e uma AppImage atualizam de ponta a ponta.
- App com versão não comparável mostra "desconhecida" e permite atualizar manualmente.
- Sem pedidos repetidos que esgotem o limite do GitHub.

**Riscos:** `.deb` cujo nome de pacote muda entre versões; AppImages que não expõem versão. Resolver por metadados guardados na instalação, com fallback para inspeção do ficheiro.

---

## Fase 4 — Publicação

**Objetivo:** tornar o projeto utilizável e publicável.

**Entregas**
- Empacotamento `.deb` para Pop!_OS, com ícone e entrada de menu.
- README com capturas, requisitos e limitações conhecidas.
- CI: build, testes, lint, release de `.deb` e AppImage da própria app.
- Licença GPL-3.0, CONTRIBUTING, template de issues.
- Nome definitivo e identidade visual (D5).
- Revisão de i18n.

**Definição de pronto**
- Instalação limpa numa máquina sem toolchain de Rust.
- Release reproduzível a partir de tag.
- Limitações documentadas de forma honesta.

---

## Ordem e dependências

```
Fase 0 ──► Fase 1 ──► Fase 2 ──► Fase 3 ──► Fase 4
   └──────────────► (o modelo App serve as duas metades)
```

A fase 0 é obrigatória para tudo. A fase 1 não depende da 2; a 2 não depende da 1 além do modelo e do helper privilegiado. Se a prioridade mudar (D1), a fase 1 e a 2 podem trocar de ordem sem refazer trabalho.

## Decisões que bloqueiam fases

| Decisão | Bloqueia |
|---------|----------|
| D1 — que metade primeiro | Ordem das fases 1 e 2 |
| D2 — apagar vs. lixo | Fase 1 |
| D3 — modelo de privilégios | Fases 1 e 2 |
| D4 — papel do Flatpak | Fase 1 |
| D5 — nome | Fase 4 |

## Registo de alterações

| Data | Alteração | Fase |
|------|-----------|------|
| 2026-10-27 | Criação do documento, fases 0–4 propostas | — |
| 2026-10-27 | Fase 0: motor de inventário (apt, Flatpak, AppImage) com 23 testes. Corrigido mapeamento de pacotes (nomes simples e `/bin`→`/usr/bin`). Inventário em 445 ms. | 0 |
| 2026-10-27 | Fase 0: janela libcosmic com pesquisa, secções e atualização manual; 8 testes de apresentação. | 0 |
| 2026-10-27 | Fase 0: menu lateral com Instaladas por mim / Sistema / Manutenção. | 0 |
| 2026-10-27 | Fase 0: classificação passou a ler `extended_states` e as dependências do desktop localmente; inventário em 266 ms. `cosmic-term` (recommend do `cosmic-session`) passa a sistema. | 0 |
| 2026-10-27 | Fase 0 (D7): separador Sistema dividido em Protegidas e Removíveis, com cadeado nas protegidas. Corrigida a leitura de relações multi-linha (`cosmic-edit`). | 0 |
| 2026-10-27 | Fase 0 (D8): interface passou a inglês com Fluent (`i18n/en/app_manager.ftl`); pronta para novos idiomas com fallback. | 0 |
| 2026-10-27 | Fase 1 (D9): linhas selecionáveis, painel de detalhe e pré-visualização de remoção com `apt-get -s`; bibliotecas partilhadas e órfãs ficam instaladas. Nada é executado. | 1 |
| 2026-10-27 | Fase 1 (D10): apps protegidas passaram para o fundo do separador Sistema, com botão *Show locked / Hide locked* e contagem. | 1 |
| 2026-10-27 | Fase 1 (D11): menu de três pontos em cada linha com *Details* e *Remove*; *Remove* abre um diálogo de confirmação com o plano real e executa só depois de confirmar (`pkexec` para apt/Flatpak de sistema, `gio trash` para AppImage). | 1 |
