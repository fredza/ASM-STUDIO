; Lire / Écrire — lit un octet sur stdin, l'affiche sur stdout
; Démontre : syscall read (0) et write (1)
; Lancer depuis un terminal : echo "X" | ./build/lire_ecrire

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
