# jibril

[English](README.md)

Ferramental curado para Linux imutável, guiado por uma árvore de catálogo.

> **Status: planejamento.** O desenho está fechado e escrito; o motor ainda não
> foi implementado. Veja [docs/vision.md](docs/vision.md) para a ideia,
> [docs/architecture.md](docs/architecture.md) para o contrato e
> [docs/decisions.md](docs/decisions.md) para o porquê de cada escolha. Os
> documentos estão em inglês, como o resto do código.

## A ideia

Uma distro baseada em imagem é ótima em *ser* um sistema e ruim em *crescer*
um. Instalar um editor, um banco ou um toolchain nunca é difícil, mas é
conhecimento: qual id de Flatpak, qual container, qual unit de Quadlet, e qual
configuração precisa vir depois. Esse conhecimento evapora entre uma
reinstalação e a próxima.

O jibril transforma isso numa árvore navegável e executável. Um catálogo TOML
declara menus e itens; cada item carrega a receita que instala, configura,
verifica e remove. O caminho do comando é o caminho na árvore:

```sh
jibril install                 # lista os grupos sob install
jibril install code            # lista os itens sob install/code
jibril install code vs-code    # executa a receita daquele item
```

O motor não sabe nada sobre Flatpak, podman ou distrobox. Ele resolve um
caminho, decide privilégio, executa os ganchos da receita em ordem, desfaz o
que precisa quando algo falha, e reporta em texto ou JSON. Todo mecanismo vive
numa receita, então um mecanismo novo custa um arquivo TOML, não uma release.

## O desenho em uma tela

- **O catálogo é a superfície de comandos.** Subcomandos são dado, não código.
  A distro ou o usuário acrescenta um grupo inteiro sem release do jibril.
- **Quatro camadas de catálogo**, em precedência crescente: embutida no
  binário, `/usr/share/jibril/catalog/`, `/etc/jibril/catalog/` e a do usuário
  em `$XDG_DATA_HOME/jibril/catalog/`.
- **Oito ganchos por receita:** `check`, `pre`, `install`, `post`, `roll-pre`,
  `roll-install`, `roll-post`, `uninstall`. A remoção é derivada dos ganchos de
  rollback quando `uninstall` não existe.
- **Sem estado.** O jibril não registra o que instalou; o `check` pergunta ao
  sistema, em paralelo e com timeout.
- **Rust, por causa da partida.** O binário é efêmero e nasce de novo a cada
  passo de navegação no menu.

## O menu

O jibril não desenha nada. O menu é uma extensão do
[Vicinae](https://vicinae.com) que consome `--json`, desenha os filhos de um
nó, mostra quais itens já estão presentes e chama install ou uninstall de forma
explícita. Outros front ends são bem-vindos, e igualmente externos.

## Onde ele se encaixa

| Projeto | Papel |
| --- | --- |
| [kuuhaku-os](https://github.com/LLawli/kuuhaku-os) | A distro: imagem Fedora bootc em que o `Containerfile` é o sistema. |
| [sora](https://github.com/LLawli/sora) | Apaga a linha entre host e container: digite o comando e ele roda, onde quer que more. |
| **jibril** | Transforma ferramental curado numa árvore executável e navegável. |
| shiro | Permissões e sandbox para boxes, Flatpaks e apps nativos sob bwrap. Ainda não começou. |

O sora responde "onde esse comando mora". O jibril responde "como essa
ferramenta chega aqui". O shiro vai responder "o que essa ferramenta pode
tocar".

O motor é portável para qualquer host com shell POSIX. A curadoria é escrita
para Fedora bootc e não finge o contrário.

## Licença

MIT.
