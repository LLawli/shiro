# shiro

[English](README.md)

Ferramental curado para Linux imutável, guiado por uma árvore de catálogo.

> **Status: 0.1.0.** O motor está completo em relação ao contrato em
> [docs/architecture.md](docs/architecture.md): o catálogo, o executor e o
> módulo de permissões. Ele **não traz receitas**: a curadoria mora numa camada
> acima dele, que neste ecossistema é o kuuhaku-os. Veja
> [catalog/README.md](catalog/README.md). Os documentos em `docs/` estão em
> inglês, como o resto do código.

## A ideia

Uma distro baseada em imagem é ótima em *ser* um sistema e ruim em *crescer*
um. Instalar um editor, um banco ou um toolchain nunca é difícil, mas é
conhecimento: qual id de Flatpak, qual container, qual unit de Quadlet, e qual
configuração precisa vir depois. Esse conhecimento evapora entre uma
reinstalação e a próxima.

O shiro transforma isso numa árvore navegável e executável. Um catálogo TOML
declara menus e itens; cada item carrega a receita que instala, configura,
verifica e remove. O caminho do comando é o caminho na árvore:

```sh
shiro install                 # lista os grupos sob install
shiro install code            # lista os itens sob install/code
shiro install code vs-code    # executa a receita daquele item
```

O motor não sabe nada sobre Flatpak, podman ou distrobox. Ele resolve um
caminho, decide privilégio, executa os ganchos da receita em ordem, desfaz o
que precisa quando algo falha, e reporta em texto ou JSON. Todo mecanismo vive
numa receita, então um mecanismo novo custa um arquivo TOML, não uma release.

## Instalação

```sh
curl -fsSL https://raw.githubusercontent.com/LLawli/shiro/main/packaging/install.sh | sh
```

Baixa o tarball da release, confere o sha256 contra o checksum publicado e
instala em `~/.local` sem root, o que sobrevive a um rebase de imagem numa
distro atômica. `PREFIX=/usr/local` muda o destino. O artefato é um binário
musl estático e não precisa de nada no host.

A partir do código, com Rust 1.88 ou mais novo:

```sh
cargo build --release    # target/release/shiro
```

Um shiro recém-instalado lista três menus vazios. Isso está certo: é um motor de
catálogo sem catálogo, e ele imprime os diretórios onde uma camada pode ser
colocada.

## Usando

Nomear um menu lista o menu; nomear um item executa a receita. Não existe verbo
`list`, porque um menu sem argumentos restantes para consumir *é* a listagem.

```sh
shiro install code --json               # o que um front end lê
shiro install code vs-code              # instala, e recusa se o check disser que já está
shiro install code vs-code --force      # pula a porteira do check, e nada além disso
shiro install code vs-code --dry-run    # imprime cada comando, sem executar nenhum
shiro install code vs-code --uninstall  # remove
```

`--keep-partial` rebaixa um rollback atômico para um por fase numa única
execução, para depurar uma receita sem pagar a reinstalação.

Ao lado da árvore existem os comandos nativos, para o que exige enxergar o
estado do próprio motor:

| Comando | Para quê |
| --- | --- |
| `shiro doctor` | Quais camadas carregaram, o que cada uma contribuiu, quais rótulos de `mechanism` o catálogo usa. |
| `shiro catalog validate` | Faz valer o schema. Rode antes de publicar uma camada. |
| `shiro catalog sources` | Qual camada contribuiu ou sobrescreveu cada nó. |
| `shiro version` | A versão, e um digest do catálogo mesclado. |
| `shiro perms …`, `shiro run …` | Permissões, abaixo. |

Os códigos de saída são contrato: `0` deu certo, `1` um gancho ou o catálogo
falhou, `2` a invocação estava errada, `3` o shiro recusou e nada aconteceu,
`4` um gancho de rollback falhou e o sistema está num estado que ninguém quis.

## Escrevendo uma camada de catálogo

Coloque arquivos `.toml` em `/usr/share/shiro/catalog/` (imagem da distro),
`/etc/shiro/catalog/` (uma máquina) ou `$XDG_DATA_HOME/shiro/catalog/` (um
usuário). Os nomes dos arquivos não importam: o caminho vem das declarações.

```toml
[[menu]]
path  = "install.code"
title = "Editores de código"
order = 20

[[item]]
path        = "install.code.vs-code"
title       = "Visual Studio Code"
mechanism   = "flatpak"        # rótulo para exibição, nunca chave de despacho
privilege   = "user"           # user | system
rollback    = "atomic"         # atomic | phase | none

[item.hooks]
check        = "flatpak info --user com.visualstudio.code"
install      = "flatpak install --user -y flathub com.visualstudio.code"
roll-install = "flatpak uninstall --user -y com.visualstudio.code"
```

O `shiro catalog validate` sai com código diferente de zero e nomeia o arquivo,
o nó e o problema. O formato completo está em
[docs/architecture.md](docs/architecture.md), seções 3 e 4.

## O desenho em uma tela

- **O catálogo é a superfície de comandos.** Subcomandos são dado, não código.
  A distro ou o usuário acrescenta um grupo inteiro sem release do shiro.
- **Quatro camadas de catálogo**, em precedência crescente: embutida no
  binário, `/usr/share/shiro/catalog/`, `/etc/shiro/catalog/` e a do usuário
  em `$XDG_DATA_HOME/shiro/catalog/`. Uma camada substitui um nó por inteiro.
- **Oito ganchos por receita:** `check`, `pre`, `install`, `post`, `roll-pre`,
  `roll-install`, `roll-post`, `uninstall`. A remoção é derivada dos ganchos de
  rollback quando `uninstall` não existe.
- **O rollback é declarado, não adivinhado:** `atomic` desfaz tudo, `phase`
  desfaz só o que falhou, `none` não desfaz nada. Um rollback que falha é um
  desfecho próprio, com código de saída próprio.
- **Sem estado.** O shiro não registra o que instalou; o `check` pergunta ao
  sistema, em paralelo, com timeout, e nunca elevado.
- **Rust, por causa da partida.** O binário é efêmero e nasce de novo a cada
  passo de navegação no menu. Uma listagem com checks roda em poucos
  milissegundos.

## Permissões

Instalar uma ferramenta e decidir o que ela pode tocar acontecem no mesmo
momento, então o shiro cuida das duas coisas. De forma modesta:

```sh
shiro perms flatpak com.brave.Browser                        # mostra os overrides
shiro perms flatpak com.brave.Browser deny filesystem=home   # aciona o flatpak override
shiro perms run brave        # o perfil, e a invocação exata do bwrap que ele gera
shiro run brave              # lança o app sob esse perfil
```

O `shiro perms` sempre nomeia o backend, então nunca se infere o que está sendo
alterado. O `shiro run` lança aplicações nativas sob bwrap conforme um perfil
declarado, e um app sem perfil roda num fallback que não concede quase nada, de
forma barulhenta: todos os namespaces separados, `/usr` e `/etc` somente
leitura, um tmpfs sobre o diretório home, o ambiente esvaziado. Ele nunca roda
sem confinamento. Os perfis têm as mesmas camadas do catálogo, e a camada do
usuário pode afrouxar um perfil, não só apertar.

```toml
# ~/.local/share/shiro/profiles/brave.toml
backend = "run"
app     = "brave"

[run]
command    = "/usr/bin/brave-browser"
network    = true
share      = ["wayland", "pipewire"]
devices    = ["dri"]
read-write = ["~/Downloads"]
```

Uma receita pode declarar `[item.permissions]` para o que instala. O motor
registra o perfil e nada além disso: gerar o wrapper ou o `.desktop` que chama
o `shiro run` é trabalho do `post` da própria receita.

Isso não é um motor de política e não tenta ser o firejail. É a menor coisa que
torna verdadeira a frase "o navegador roda em sandbox mesmo não sendo Flatpak".

## O menu

O shiro não desenha nada. O menu é uma extensão do
[Vicinae](https://vicinae.com) que consome `--json`, desenha os filhos de um
nó, mostra quais itens já estão presentes e chama install ou uninstall de forma
explícita. Outros front ends são bem-vindos, e igualmente externos.

O payload carrega um campo `schema` e é versionado: uma quebra nele é uma quebra
do shiro, e fica registrada no [CHANGELOG.md](CHANGELOG.md).

## Onde ele se encaixa

| Projeto | Papel |
| --- | --- |
| [kuuhaku-os](https://github.com/LLawli/kuuhaku-os) | A distro: imagem Fedora bootc em que o `Containerfile` é o sistema. |
| [sora](https://github.com/LLawli/sora) | Apaga a linha entre host e container: digite o comando e ele roda, onde quer que more. |
| **shiro** | Transforma ferramental curado numa árvore executável e navegável, e é dono do que cada ferramenta pode tocar. |

O sora responde "onde esse comando mora". O shiro responde "como essa
ferramenta chega aqui, e o que ela pode tocar".

O motor é portável para qualquer host com shell POSIX. A curadoria é escrita
para Fedora bootc e não finge o contrário.

## Contribuindo

Leia o [CLAUDE.md](CLAUDE.md) primeiro: ele guarda as regras fáceis de perder de
vista e caras de perder de vista, começando pela que sustenta o desenho inteiro,
a de que o executor de receitas nunca aprende um mecanismo. O
[docs/decisions.md](docs/decisions.md) registra o que foi decidido e o que foi
rejeitado, para que nada já investigado seja proposto de novo do zero.

## Licença

MIT.
