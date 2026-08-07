; Lire / Écrire — lit un octet sur stdin, l'affiche sur stdout
; Démontre : syscall read (0) et write (1)
;
; Lancez-le ici même : au « read », la barre d'état passe à « En attente
; d'entrée » et le chevron du panneau Console s'allume. Tapez un caractère,
; Entrée : le programme repart, et son « write » ressort dans cette console.

section .bss
    buf resb 1          ; tampon d'un octet

section .text
    global _start

_start:
    ; read(0, buf, 1)
    xor rax, rax        ; syscall : read
    xor rdi, rdi        ; fd = stdin
    mov rsi, buf        ; adresse du tampon
    mov rdx, 1          ; lire 1 octet
    syscall
    ; rax = nombre d'octets lus (0 si EOF)

    test rax, rax
    jz .fin             ; EOF : quitter sans afficher

    ; write(1, buf, 1)
    mov rax, 1          ; syscall : write
    mov rdi, 1          ; fd = stdout
    mov rsi, buf
    mov rdx, 1
    syscall

.fin:
    mov rax, 60         ; exit(0)
    xor rdi, rdi
    syscall
