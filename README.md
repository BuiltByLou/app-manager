# App Manager

Gestor de aplicações nativo para desktops Linux (Pop!_OS / COSMIC), escrito em
Rust com libcosmic. Lista o que está instalado, remove sem partir o sistema e
instala ou actualiza apps a partir de releases do GitHub.

![estado](https://img.shields.io/badge/estado-beta-orange)
![licença](https://img.shields.io/badge/licen%C3%A7a-GPL--3.0-blue)

## O que faz

- **Inventário** de apps apt (via `.desktop` + dpkg), Flatpak e AppImage, com
  ícones reais e versão.
- **Remoção segura**: só remove a app. Bibliotecas partilhadas e pacotes órfãos
  ficam instalados, e o que sobra é enviado para o lixo (`gio trash`).
- **Instalação a partir de um repositório GitHub**: cola o URL, escolhe o build
  compatível (`.deb`, `.AppImage`, `.flatpak`) e a app fica pronta a usar, com
  entrada no lançador e ícone.
- **Actualizações** das apps instaladas por esta via, comparando com a última
  release e mantendo o mesmo formato.
- **Sem password no código**: a autorização sudo é pedida uma vez por sessão.

## Como classifica as apps

Cada linha mostra duas tags:

| Tag | Significado | Ligar a um repo? |
| --- | --- | --- |
| **Hub** | apt ou Flatpak actualiza-a a partir de um repositório remoto | Não precisa |
| **GitHub** | instalada por esta app a partir de uma release | Sim, editável |
| **Local** | ficheiro local, sem repositório por trás (AppImage, `.deb` de ficheiro) | Sim |

A classificação **Hub** não é só "é um pacote apt": pergunta-se ao
`apt-cache policy` se existe uma origem remota, e ao Flatpak se o `origin` é um
remote com URL de rede. Um `.deb` instalado de um ficheiro local aparece como
**Local**, tal como um AppImage.

O tipo de instalação (removível sem risco) é outra dimensão: **User**,
**Pre-installed** ou **System**, com as apps essenciais bloqueadas no fundo da
lista.

## Instalar

```bash
sudo apt install ./app-manager_0.0.7_amd64.deb
```

## Compilar

Requer uma toolchain Rust recente e as dependências de sistema do libcosmic
(`libxkbcommon-dev`, entre outras).

```bash
cargo build --release -p appmgr
cargo test --workspace
./packaging/build-deb.sh   # deixa o pacote em dist/
```

## Estrutura

- `crates/appmgr-core` — inventário, remoção, instaladores, GitHub, registo de
  origens. Sem UI.
- `crates/appmgr-ui` — interface libcosmic, vista de lista com tags e filtros.
- `packaging/build-deb.sh` — monta o `.deb` com `dpkg-deb`.

## Licença

GPL-3.0-only. Ver [LICENSE](LICENSE).
