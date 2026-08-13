# shiro

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

## O desenho em uma tela

- **O catálogo é a superfície de comandos.** Subcomandos são dado, não código.
  A distro ou o usuário acrescenta um grupo inteiro sem release do shiro.
- **Quatro camadas de catálogo**, em precedência crescente: embutida no
  binário, `/usr/share/shiro/catalog/`, `/etc/shiro/catalog/` e a do usuário
  em `$XDG_DATA_HOME/shiro/catalog/`.
- **Oito ganchos por receita:** `check`, `pre`, `install`, `post`, `roll-pre`,
  `roll-install`, `roll-post`, `uninstall`. A remoção é derivada dos ganchos de
  rollback quando `uninstall` não existe.
- **Sem estado.** O shiro não registra o que instalou; o `check` pergunta ao
  sistema, em paralelo e com timeout.
- **Rust, por causa da partida.** O binário é efêmero e nasce de novo a cada
  passo de navegação no menu.

## Permissões

Instalar uma ferramenta e decidir o que ela pode tocar acontecem no mesmo
momento, então o shiro cuida das duas coisas. De forma modesta:

```sh
shiro perms flatpak brave --nofilesystem=home   # aciona o flatpak override
shiro perms run brave                           # o perfil bwrap de um app nativo
shiro run brave                                 # lança o app sob esse perfil
```

O `shiro perms` sempre nomeia o backend, então nunca se infere o que está sendo
alterado. O `shiro run` lança aplicações nativas sob bwrap conforme um perfil
declarado, e um app sem perfil roda num fallback que não concede quase nada, de
forma barulhenta. Os perfis têm as mesmas camadas do catálogo.

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

## Licença

MIT.
