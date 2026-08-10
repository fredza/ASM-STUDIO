;@titre Fonction soustraire (cours 8.1)
;@enonce Écris une fonction « soustraire » qui prend deux nombres dans RDI et RSI
;@enonce et renvoie RDI - RSI dans RAX — la convention d'appel du chapitre 8.
;@enonce Appelle-la avec 50 et 8 : R14 doit recueillir 42.
;@requis call
;@requis ret
;@attendu r14 == 42
;@attendu exit == 42

section .text
    global _start

soustraire:
    ; TODO : mettre RDI dans RAX, puis lui soustraire RSI
    ; TODO : et rendre la main  (« ret »)

_start:
    mov rdi, 50
    mov rsi, 8
    ; TODO : appeler soustraire
    mov r14, rax                ; le résultat, hors d'atteinte du « mov rax, 60 »

    mov rax, 60
    mov rdi, r14
    syscall
