%include "asmstd.inc"

section .data
    s1      db "Bonjour Monde", 0
    s2      db "Monde", 0
    s3      db "bonjour", 0
    buf     times 64 db 0
    tab     dq 5, 3, 9, 1, 7
    tab2    dq 5, 3, 9, 1, 7
    n_ok    db "resultat", 0

section .text
    global _start
_start:
    ; --- caractères ---
    mov rdi, '7'
    call asm.is_digit
    push rax                    ; attendu 1
    mov rdi, 'x'
    call asm.is_digit
    push rax                    ; attendu 0
    mov rdi, 'a'
    call asm.to_upper
    push rax                    ; attendu 'A' = 65
    mov rdi, ' '
    call asm.is_space
    push rax                    ; attendu 1

    ; --- chaînes ---
    lea rdi, [s1]
    lea rsi, [s2]
    call asm.strstr
    lea rbx, [s1]
    sub rax, rbx
    push rax                    ; attendu 8 (index de "Monde")

    lea rdi, [s1]
    mov sil, 'o'
    call asm.str_count
    push rax                    ; attendu 3 (B-o-njour M-o-nde -> o,o,o)

    lea rdi, [s1]
    mov sil, 'M'
    call asm.strchr
    lea rbx, [s1]
    sub rax, rbx
    push rax                    ; attendu 8

    lea rdi, [buf]
    lea rsi, [s3]
    call asm.strcpy
    lea rdi, [buf]
    call asm.str_upper
    lea rdi, [buf]
    mov sil, 'B'
    call asm.strchr
    test rax, rax
    setnz al
    movzx rax, al
    push rax                    ; attendu 1 (str_upper a bien majusculé)

    lea rdi, [buf]
    call asm.str_reverse
    movzx rax, byte [buf]
    push rax                    ; attendu 'R' = 82 (RUOJNOB)

    ; --- maths ---
    mov rdi, -42
    call asm.abs
    push rax                    ; attendu 42
    mov rdi, 48
    mov rsi, 18
    call asm.gcd
    push rax                    ; attendu 6
    mov rdi, 4
    mov rsi, 6
    call asm.lcm
    push rax                    ; attendu 12
    mov rdi, 2
    mov rsi, 10
    call asm.pow
    push rax                    ; attendu 1024
    mov rdi, 17
    mov rsi, 5
    call asm.divmod
    push rdx                    ; attendu 2 (reste)
    push rax                    ; attendu 3 (quotient)
    mov rdi, 7
    mov rsi, 0
    call asm.divmod
    push rax                    ; attendu 0 (division par zero neutralisee)
    mov rdi, 3
    mov rsi, 9
    call asm.max
    push rax                    ; attendu 9
    mov rdi, 20
    mov rsi, 0
    mov rdx, 10
    call asm.clamp
    push rax                    ; attendu 10

    ; --- tableaux ---
    lea rdi, [tab]
    mov rsi, 5
    call asm.arr_sum
    push rax                    ; attendu 25
    lea rdi, [tab]
    mov rsi, 5
    call asm.arr_max
    push rax                    ; attendu 9
    lea rdi, [tab]
    mov rsi, 5
    call asm.arr_min
    push rax                    ; attendu 1
    lea rdi, [tab]
    mov rsi, 5
    mov rdx, 9
    call asm.arr_find
    push rax                    ; attendu 2
    lea rdi, [tab]
    mov rsi, 5
    call asm.arr_sort
    mov rax, [tab]
    push rax                    ; attendu 1 (plus petit en tete)
    mov rax, [tab + 32]
    push rax                    ; attendu 9 (plus grand en queue)
    lea rdi, [tab2]
    mov rsi, 5
    call asm.arr_reverse
    mov rax, [tab2]
    push rax                    ; attendu 7

    ; --- memoire ---
    lea rdi, [s1]
    lea rsi, [s1]
    mov rdx, 5
    call asm.memcmp
    push rax                    ; attendu 0
    lea rdi, [s1]
    lea rsi, [s3]
    mov rdx, 3
    call asm.strncmp
    test rax, rax
    setnz al
    movzx rax, al
    push rax                    ; attendu 1 (differents : 'Bon' vs 'bon')

    ; Affiche toutes les valeurs empilees, de la derniere a la premiere.
    mov r14, 24                 ; nombre de push
.print_lp:
    pop rdi
    call asm.print_num
    call asm.newline
    dec r14
    jnz .print_lp

    mov rax, 60
    xor rdi, rdi
    syscall
