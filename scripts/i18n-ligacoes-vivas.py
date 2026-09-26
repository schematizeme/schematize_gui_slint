#!/usr/bin/env python3
"""Guard: toda ligacao de rotulo deste hub tem de ser DESENHADA por alguma tela.

O QUE: compara os `l.set_<prop>(...)` de `src/i18nbind.rs` com os `L.<prop-com-hifen>`
usados nos `.slint`. Reprova por CODIGO DE SAIDA, nunca por texto.

POR QUE EXISTE, medido em 2026-09-26: 77 das 356 ligacoes nao eram lidas por `.slint`
nenhum, e 45 delas eram do `vps` e do `ssh` — sobras da extracao do deployer, que levou
as TELAS e deixou os rotulos para tras. Ninguem notou porque ligacao morta nao da erro:
ela grava uma propriedade que existe, e a propriedade simplesmente nao e desenhada.

O custo nao e o byte. E que cada uma dessas linhas e uma chave que alguem traduziria em
20 idiomas — trabalho inteiro sobre tela que nao existe mais.

POR QUE E SCRIPT E NAO `#[test]`: a varredura precisa ler TODOS os `.slint` do diretorio,
e o `include_str!` do Rust nao aceita glob. Um teste com a lista de arquivos escrita a mao
apodreceria no primeiro arquivo novo — que e exatamente a falha que ele existe pra pegar.

ONDE: `.schematize/overdev/gate.sh` e o CI deste repo.
"""
import pathlib
import re
import sys

RAIZ = pathlib.Path(__file__).resolve().parent.parent


def ligacoes(fonte: str) -> list[str]:
    """As propriedades que o Rust grava, so no codigo de PRODUCAO.

    Os testes ficam de fora porque eles NOMEIAM propriedades para procura-las, e um
    varredor que as contasse acharia viva toda propriedade que um teste menciona.
    """
    producao = fonte.split("#[cfg(test)]")[0]
    return sorted(set(re.findall(r"l\.set_([a-z0-9_]+)\(", producao)))


def usadas(telas: str) -> set[str]:
    """As propriedades que algum `.slint` DESENHA, ja com `_` no lugar do `-`."""
    return {m.replace("-", "_") for m in re.findall(r"L\.([a-z0-9-]+)", telas)}


def main() -> int:
    fonte = (RAIZ / "src" / "i18nbind.rs").read_text(encoding="utf-8")
    arquivos = sorted((RAIZ / "ui").glob("*.slint"))
    if not arquivos:
        print("ERRO: nenhum .slint em ui/ — o varredor esta cego", file=sys.stderr)
        return 2
    telas = "\n".join(a.read_text(encoding="utf-8") for a in arquivos)

    todas = ligacoes(fonte)
    vivas = usadas(telas)
    if not todas:
        print("ERRO: nenhuma ligacao encontrada — o varredor esta cego", file=sys.stderr)
        return 2
    if not vivas:
        print("ERRO: nenhum uso `L.<prop>` encontrado — o varredor esta cego", file=sys.stderr)
        return 2

    mortas = [p for p in todas if p not in vivas]
    print(f"{len(todas)} ligacoes · {len(mortas)} sem tela que as desenhe")
    if mortas:
        for p in mortas:
            print(f"  MORTA  l.set_{p}()  — nenhum .slint usa L.{p.replace('_', '-')}")
        print(
            "\nLigacao sem tela e rotulo de tela que nao existe mais. Tire a linha do "
            "`i18nbind.rs` E a propriedade do `ui/i18n.slint`.",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
