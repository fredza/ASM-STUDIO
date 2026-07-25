; asmstd.inc — petite bibliothèque standard pédagogique pour ASM Studio.
;
; Elle masque les NUMÉROS de syscalls derrière des noms lisibles.
; Convention d'appel : placez les arguments dans rdi, rsi, rdx (comme pour un
; vrai syscall), puis `call asm.write`. Le résultat est renvoyé dans rax
; (valeur négative = -errno en cas d'erreur).
;
; Exemple :
;     mov rdi, 1          ; fd = stdout
;     mov rsi, message
;     mov rdx, longueur
;     call asm.write
;
; `asm.write`, `asm.exit`, … sont des labels locaux du label global `asm`.

%ifndef ASMSTD_INC
%define ASMSTD_INC

section .text

asm:                        ; label parent (les fonctions sont ses labels locaux)
.read:                      ; (rdi=fd, rsi=buf, rdx=count) -> rax = octets lus
    mov     rax, 0
    syscall
    ret
.write:                     ; (rdi=fd, rsi=buf, rdx=count) -> rax = octets écrits
    mov     rax, 1
    syscall
    ret
.open:                      ; (rdi=path, rsi=flags, rdx=mode) -> rax = fd
    mov     rax, 2
    syscall
    ret
.close:                     ; (rdi=fd)
    mov     rax, 3
    syscall
    ret
.mkdir:                     ; (rdi=path, rsi=mode)
    mov     rax, 83
    syscall
    ret
.fork:                      ; () -> rax = pid (0 dans l'enfant)
    mov     rax, 57
    syscall
    ret
.execve:                    ; (rdi=path, rsi=argv, rdx=envp)
    mov     rax, 59
    syscall
    ret
.exit:                      ; (rdi=code) — ne revient jamais
    mov     rax, 60
    syscall
    ret

%endif
