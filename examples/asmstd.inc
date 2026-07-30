; asmstd.inc — bibliothèque standard pédagogique pour ASM Studio
; Linux x86-64 (NASM) · syscalls + utilitaires courants
;
; === Convention d'appel (Linux x86-64 / System V ABI) =================
;   Arguments  : rdi  rsi  rdx  rcx  r8   r9
;   Retour     : rax  (valeur négative = -errno pour les wrappers syscall)
;   Préservés  : rbx  rbp  r12..r15  (callee-saved ; restaurés si modifiés)
;   Écrasés    : rax  rcx  rdx  rsi  rdi  r8..r11  (caller-saved)
;
; === 4e argument syscall vs appel de fonction =========================
;   En C/ABI fonction, le 4e argument va dans rcx.
;   Mais le noyau Linux attend le 4e argument dans r10 (pas rcx !).
;   Les wrappers asm.mmap, asm.wait4, asm.send… font mov r10, rcx
;   pour vous : passez vos 4 arguments normalement dans rdi/rsi/rdx/rcx.
;
; === Utilisation ======================================================
;   %include "asmstd.inc"       ; une seule inclusion suffit (%ifndef guard)
;
;   ; Syscall brut :
;   mov  rdi, 1                 ; fd = stdout
;   lea  rsi, [msg]
;   mov  rdx, len
;   call asm.write
;
;   ; Helper haut niveau :
;   lea  rdi, [msg]             ; chaîne terminée par 0
;   call asm.print              ; écrit sur stdout (calcule la longueur seul)
;
;   mov  rdi, 42
;   call asm.print_num          ; affiche "42\n" sur stdout
;
;   xor  rdi, rdi
;   call asm.exit

%ifndef ASMSTD_INC
%define ASMSTD_INC

section .text

; ======================================================================
;  WRAPPERS SYSCALL
;  Chaque wrapper = numéro dans rax + syscall + ret.
;  Passez les arguments dans rdi, rsi, rdx, rcx, r8, r9 (ordre standard).
; ======================================================================

asm:            ; label parent — les fonctions sont ses labels locaux (.xxx)

; ---- Entrées / Sorties -----------------------------------------------

.read:          ; (rdi=fd, rsi=buf, rdx=count) -> rax = octets lus, ou -errno
    mov     rax, 0
    syscall
    ret

.write:         ; (rdi=fd, rsi=buf, rdx=count) -> rax = octets écrits, ou -errno
    mov     rax, 1
    syscall
    ret

.open:          ; (rdi=path, rsi=flags, rdx=mode) -> rax = fd, ou -errno
    ;   flags : O_RDONLY=0  O_WRONLY=1  O_RDWR=2  O_CREAT=0x40  O_TRUNC=0x200
    ;   mode  : ex. 0o644 = rw-r--r--
    mov     rax, 2
    syscall
    ret

.close:         ; (rdi=fd) -> rax = 0 ou -errno
    mov     rax, 3
    syscall
    ret

.lseek:         ; (rdi=fd, rsi=offset, rdx=whence) -> rax = nouvelle position
    ;   whence : 0=SEEK_SET  1=SEEK_CUR  2=SEEK_END
    mov     rax, 8
    syscall
    ret

.access:        ; (rdi=path, rsi=mode) -> rax = 0 si accessible, -errno sinon
    ;   mode : 0=F_OK(existe)  1=X_OK  2=W_OK  4=R_OK  (combinables par OR)
    mov     rax, 21
    syscall
    ret

.dup:           ; (rdi=fd) -> rax = nouveau fd (premier disponible)
    mov     rax, 32
    syscall
    ret

.dup2:          ; (rdi=oldfd, rsi=newfd) -> rax = newfd
    mov     rax, 33
    syscall
    ret

.pipe:          ; (rdi=pipefd[2]) -> rax = 0 ou -errno
    ;   pipefd[0] = extrémité lecture  pipefd[1] = extrémité écriture
    mov     rax, 22
    syscall
    ret

.fcntl:         ; (rdi=fd, rsi=cmd, rdx=arg) -> rax
    ;   cmd : F_GETFL=3  F_SETFL=4  F_GETFD=1  F_SETFD=2
    mov     rax, 72
    syscall
    ret

.ioctl:         ; (rdi=fd, rsi=request, rdx=arg) -> rax
    mov     rax, 16
    syscall
    ret

.poll:          ; (rdi=fds*, rsi=nfds, rdx=timeout_ms) -> rax = nb événements
    ;   struct pollfd : dword fd, word events, word revents
    ;   events : 1=POLLIN  4=POLLOUT  timeout_ms=-1 = attente infinie
    mov     rax, 7
    syscall
    ret

; ---- Fichiers et répertoires -----------------------------------------

.stat:          ; (rdi=path, rsi=statbuf*) -> rax = 0 ou -errno
    mov     rax, 4
    syscall
    ret

.fstat:         ; (rdi=fd, rsi=statbuf*) -> rax = 0 ou -errno
    mov     rax, 5
    syscall
    ret

.lstat:         ; (rdi=path, rsi=statbuf*) -> rax  (ne suit pas les liens symboliques)
    mov     rax, 6
    syscall
    ret

.unlink:        ; (rdi=path) -> rax = 0 ou -errno  — supprime un fichier
    mov     rax, 87
    syscall
    ret

.rename:        ; (rdi=oldpath, rsi=newpath) -> rax = 0 ou -errno
    mov     rax, 82
    syscall
    ret

.mkdir:         ; (rdi=path, rsi=mode) -> rax = 0 ou -errno
    mov     rax, 83
    syscall
    ret

.rmdir:         ; (rdi=path) -> rax = 0 ou -errno
    mov     rax, 84
    syscall
    ret

.getcwd:        ; (rdi=buf, rsi=size) -> rax = pointeur sur buf, ou -errno
    mov     rax, 79
    syscall
    ret

.chdir:         ; (rdi=path) -> rax = 0 ou -errno
    mov     rax, 80
    syscall
    ret

.link:          ; (rdi=oldpath, rsi=newpath) -> rax = 0 ou -errno  — lien physique
    mov     rax, 86
    syscall
    ret

.symlink:       ; (rdi=target, rsi=linkpath) -> rax = 0 ou -errno
    mov     rax, 88
    syscall
    ret

.readlink:      ; (rdi=path, rsi=buf, rdx=bufsiz) -> rax = octets écrits, ou -errno
    mov     rax, 89
    syscall
    ret

.chmod:         ; (rdi=path, rsi=mode) -> rax = 0 ou -errno
    mov     rax, 90
    syscall
    ret

.truncate:      ; (rdi=path, rsi=length) -> rax = 0 ou -errno
    mov     rax, 76
    syscall
    ret

.ftruncate:     ; (rdi=fd, rsi=length) -> rax = 0 ou -errno
    mov     rax, 77
    syscall
    ret

.getdents64:    ; (rdi=fd, rsi=buf, rdx=count) -> rax = octets lus (0 = fin)
    ;   Lire un répertoire ouvert avec O_RDONLY|O_DIRECTORY.
    ;   Chaque entrée = struct linux_dirent64 (longueur variable).
    mov     rax, 217
    syscall
    ret

; ---- Processus -------------------------------------------------------

.fork:          ; () -> rax = PID enfant dans le parent, 0 dans l'enfant
    mov     rax, 57
    syscall
    ret

.execve:        ; (rdi=path, rsi=argv[], rdx=envp[]) — remplace le processus
    ;   argv et envp sont des tableaux de pointeurs terminés par NULL.
    mov     rax, 59
    syscall
    ret

.wait4:         ; (rdi=pid, rsi=status*, rdx=options, rcx=rusage*) -> rax = pid
    ;   rdi=-1 attend n'importe quel enfant. rcx=0 si rusage non utilisé.
    ;   Note interne : 4e arg syscall = r10 ← le wrapper effectue la conversion.
    mov     r10, rcx
    mov     rax, 61
    syscall
    ret

.waitpid:       ; (rdi=pid, rsi=status*, rdx=options) -> rax = pid ou -errno
    ;   Raccourci de wait4 sans rusage. rdi=-1 = attendre tout enfant.
    xor     r10, r10
    mov     rax, 61
    syscall
    ret

.exit:          ; (rdi=code) — termine ce thread (ne revient jamais)
    mov     rax, 60
    syscall
    ret

.exit_group:    ; (rdi=code) — termine tous les threads du processus
    mov     rax, 231
    syscall
    ret

.getpid:        ; () -> rax = PID du processus courant
    mov     rax, 39
    syscall
    ret

.getppid:       ; () -> rax = PID du processus parent
    mov     rax, 110
    syscall
    ret

.kill:          ; (rdi=pid, rsi=signal) -> rax = 0 ou -errno
    ;   Signaux courants : 1=SIGHUP  2=SIGINT  9=SIGKILL  15=SIGTERM
    mov     rax, 62
    syscall
    ret

.nanosleep:     ; (rdi=timespec_req*, rsi=timespec_rem*) -> rax = 0 ou -errno
    ;   struct timespec { qword tv_sec; qword tv_nsec; }
    ;   rsi peut être 0 si le temps résiduel n'est pas nécessaire.
    mov     rax, 35
    syscall
    ret

; ---- Mémoire ---------------------------------------------------------

.mmap:          ; (rdi=addr, rsi=len, rdx=prot, rcx=flags, r8=fd, r9=off) -> rax
    ;   addr=0 → le noyau choisit l'adresse.  fd=-1 si mapping anonyme.
    ;   prot  : 0=NONE  1=READ  2=WRITE  4=EXEC  (combinables par OR)
    ;   flags : 1=SHARED  2=PRIVATE  0x20=ANONYMOUS
    ;   Exemple courant : rdx=3 (R|W), rcx=0x22 (PRIVATE|ANON), r8=-1, r9=0
    ;   Note interne : 4e arg syscall = r10 ← le wrapper effectue la conversion.
    mov     r10, rcx
    mov     rax, 9
    syscall
    ret

.munmap:        ; (rdi=addr, rsi=len) -> rax = 0 ou -errno
    mov     rax, 11
    syscall
    ret

.mprotect:      ; (rdi=addr, rsi=len, rdx=prot) -> rax = 0 ou -errno
    mov     rax, 10
    syscall
    ret

.brk:           ; (rdi=addr) -> rax = nouvelle limite du segment data
    ;   rdi=0 → renvoie l'adresse courante sans la modifier (mode query).
    ;   Augmentez la valeur renvoyée pour allouer de la mémoire brute.
    mov     rax, 12
    syscall
    ret

.mremap:        ; (rdi=old_addr, rsi=old_size, rdx=new_size, rcx=flags) -> rax
    ;   flags : 1=MAYMOVE  2=FIXED
    mov     r10, rcx
    mov     rax, 25
    syscall
    ret

; ---- Identité --------------------------------------------------------

.getuid:        ; () -> rax = UID réel de l'utilisateur
    mov     rax, 102
    syscall
    ret

.getgid:        ; () -> rax = GID réel
    mov     rax, 104
    syscall
    ret

.geteuid:       ; () -> rax = UID effectif (peut différer après setuid)
    mov     rax, 107
    syscall
    ret

.getegid:       ; () -> rax = GID effectif
    mov     rax, 108
    syscall
    ret

; ---- Temps -----------------------------------------------------------

.gettimeofday:  ; (rdi=timeval*, rsi=timezone*) -> rax = 0 ou -errno
    ;   struct timeval { qword tv_sec; qword tv_usec; }  rsi peut être 0.
    mov     rax, 96
    syscall
    ret

.clock_gettime: ; (rdi=clockid, rsi=timespec*) -> rax = 0 ou -errno
    ;   clockid : 0=CLOCK_REALTIME  1=CLOCK_MONOTONIC  4=CLOCK_MONOTONIC_RAW
    ;   struct timespec { qword tv_sec; qword tv_nsec; }
    mov     rax, 228
    syscall
    ret

; ---- Réseau ----------------------------------------------------------

.socket:        ; (rdi=domain, rsi=type, rdx=protocol) -> rax = fd ou -errno
    ;   domain : 2=AF_INET  10=AF_INET6  1=AF_UNIX
    ;   type   : 1=SOCK_STREAM(TCP)  2=SOCK_DGRAM(UDP)
    mov     rax, 41
    syscall
    ret

.bind:          ; (rdi=fd, rsi=sockaddr*, rdx=addrlen) -> rax = 0 ou -errno
    mov     rax, 49
    syscall
    ret

.listen:        ; (rdi=fd, rsi=backlog) -> rax = 0 ou -errno
    mov     rax, 50
    syscall
    ret

.accept:        ; (rdi=fd, rsi=sockaddr*, rdx=addrlen*) -> rax = fd client
    mov     rax, 43
    syscall
    ret

.connect:       ; (rdi=fd, rsi=sockaddr*, rdx=addrlen) -> rax = 0 ou -errno
    mov     rax, 42
    syscall
    ret

.send:          ; (rdi=fd, rsi=buf, rdx=len, rcx=flags) -> rax = octets envoyés
    mov     r10, rcx
    mov     rax, 44
    syscall
    ret

.recv:          ; (rdi=fd, rsi=buf, rdx=len, rcx=flags) -> rax = octets reçus
    mov     r10, rcx
    mov     rax, 45
    syscall
    ret

.shutdown:      ; (rdi=fd, rsi=how) -> rax = 0 ou -errno
    ;   how : 0=SHUT_RD  1=SHUT_WR  2=SHUT_RDWR
    mov     rax, 48
    syscall
    ret

.setsockopt:    ; (rdi=fd, rsi=level, rdx=optname, rcx=optval*, r8=optlen) -> rax
    ;   Exemple : SOL_SOCKET=1, SO_REUSEADDR=2 → activer la réutilisation du port
    mov     r10, rcx
    mov     rax, 54
    syscall
    ret

; ======================================================================
;  FONCTIONS UTILITAIRES
;
;  Ces fonctions effectuent du travail en assembleur pur (pas de simple
;  mov rax, N / syscall).  Elles respectent l'ABI System V x86-64 :
;  les registres callee-saved (rbx, rbp, r12..r15) sont sauvegardés
;  et restaurés si utilisés.
; ======================================================================

; ---- Chaînes et mémoire ----------------------------------------------

.strlen:        ; (rdi=ptr) -> rax = longueur (octets avant le '\0' final)
    xor     rax, rax
.strlen_lp:
    cmp     byte [rdi + rax], 0
    je      .strlen_done
    inc     rax
    jmp     .strlen_lp
.strlen_done:
    ret

.strcpy:        ; (rdi=dst, rsi=src) -> rax = dst
    ;   Copie src dans dst, nul terminal inclus. Retourne dst.
    mov     rax, rdi
.strcpy_lp:
    mov     cl, byte [rsi]
    mov     byte [rdi], cl
    inc     rdi
    inc     rsi
    test    cl, cl
    jnz     .strcpy_lp
    ret

.strcmp:        ; (rdi=a, rsi=b) -> rax : <0 si a<b, 0 si a==b, >0 si a>b
.strcmp_lp:
    mov     al, byte [rdi]
    mov     cl, byte [rsi]
    cmp     al, cl
    jne     .strcmp_ne
    test    al, al
    jz      .strcmp_eq
    inc     rdi
    inc     rsi
    jmp     .strcmp_lp
.strcmp_eq:
    xor     rax, rax
    ret
.strcmp_ne:
    movzx   rax, al
    movzx   rcx, cl
    sub     rax, rcx
    ret

.memcpy:        ; (rdi=dst, rsi=src, rdx=n) -> rax = dst
    ;   Copie n octets de src vers dst. Les zones ne doivent pas se chevaucher.
    mov     rax, rdi
    test    rdx, rdx
    jz      .memcpy_done
.memcpy_lp:
    mov     cl, byte [rsi]
    mov     byte [rdi], cl
    inc     rdi
    inc     rsi
    dec     rdx
    jnz     .memcpy_lp
.memcpy_done:
    ret

.memset:        ; (rdi=ptr, rsi=val, rdx=n) -> rax = ptr
    ;   Remplit n octets avec l'octet bas de rsi.
    mov     rax, rdi
    test    rdx, rdx
    jz      .memset_done
    mov     cl, sil
.memset_lp:
    mov     byte [rdi], cl
    inc     rdi
    dec     rdx
    jnz     .memset_lp
.memset_done:
    ret

; ---- Affichage -------------------------------------------------------

.print:         ; (rdi=ptr) -> rax = octets écrits sur stdout
    ;   Écrit la chaîne terminée par '\0' sur fd 1 (stdout).
    push    rdi
    call    asm.strlen      ; rax = longueur (rdi inchangé dans strlen)
    mov     rdx, rax        ; count
    pop     rsi             ; buf (récupère rdi sauvegardé)
    mov     rdi, 1          ; fd = stdout
    mov     rax, 1
    syscall
    ret

.println:       ; (rdi=ptr) — imprime la chaîne puis '\n' sur stdout
    call    asm.print
    lea     rsi, [rel _asmstd_nl]
    mov     rdi, 1
    mov     rdx, 1
    mov     rax, 1
    syscall
    ret

.newline:       ; () — imprime '\n' sur stdout
    lea     rsi, [rel _asmstd_nl]
    mov     rdi, 1
    mov     rdx, 1
    mov     rax, 1
    syscall
    ret

.print_stderr:  ; (rdi=ptr) — comme asm.print mais sur stderr (fd 2)
    push    rdi
    call    asm.strlen
    mov     rdx, rax
    pop     rsi
    mov     rdi, 2
    mov     rax, 1
    syscall
    ret

; ---- Conversions entier <-> chaîne ----------------------------------

.itoa:  ; (rdi=val i64, rsi=buf, rdx=base 2..36) -> rax = longueur écrite
    ;   Écrit la représentation de val dans buf (sans '\0' final).
    ;   Base 10 : supporte les négatifs.  Autres bases : non-signé (u64).
    ;   Chiffres ≥ 10 → lettres majuscules (A..Z).
    ;   buf doit avoir ≥ 66 octets (cas base 2, 64 bits + signe).
    ;
    ;   Algorithme : division répétée → chiffres en ordre inverse dans buf,
    ;   puis inversion sur place → ordre correct.
    push    r12
    push    r13
    push    r14
    mov     r12, rdi        ; val courante
    mov     r13, rsi        ; buf (début)
    mov     r14, rdx        ; base
    mov     rcx, rsi        ; curseur d'écriture (commence au début du buf)
    ; Signe (base 10 uniquement).
    xor     r8, r8
    test    r12, r12
    jns     .itoa_pos
    cmp     r14, 10
    jne     .itoa_pos
    mov     r8, 1
    neg     r12
.itoa_pos:
    ; Extraction des chiffres par divisions successives (LSB en premier).
.itoa_lp:
    xor     rdx, rdx
    mov     rax, r12
    div     r14             ; quotient → rax, reste → rdx
    mov     r12, rax
    cmp     rdx, 9
    jle     .itoa_dec
    add     dl, 'A' - 10
    jmp     .itoa_wr
.itoa_dec:
    add     dl, '0'
.itoa_wr:
    mov     byte [rcx], dl
    inc     rcx
    test    r12, r12
    jnz     .itoa_lp
    ; Signe '-'.
    test    r8, r8
    jz      .itoa_rev
    mov     byte [rcx], '-'
    inc     rcx
.itoa_rev:
    ; Inversion sur place : [r13 .. rcx-1].
    mov     rdi, r13
    lea     rsi, [rcx - 1]
.itoa_rlp:
    cmp     rdi, rsi
    jge     .itoa_rdone
    mov     al, byte [rdi]
    mov     ah, byte [rsi]
    mov     byte [rdi], ah
    mov     byte [rsi], al
    inc     rdi
    dec     rsi
    jmp     .itoa_rlp
.itoa_rdone:
    mov     rax, rcx
    sub     rax, r13        ; longueur = curseur final - buf
    pop     r14
    pop     r13
    pop     r12
    ret

.atoi:  ; (rdi=ptr, rsi=base 2..36) -> rax = valeur i64
    ;   Ignore les espaces en début de chaîne.
    ;   Supporte le signe '-' (en base 10).
    ;   S'arrête au premier caractère non valide pour la base.
    xor     rax, rax        ; accumulateur
    xor     r8, r8          ; signe négatif
    ; Espaces initiaux.
.atoi_sp:
    cmp     byte [rdi], ' '
    jne     .atoi_sgn
    inc     rdi
    jmp     .atoi_sp
.atoi_sgn:
    cmp     byte [rdi], '-'
    jne     .atoi_lp
    inc     r8
    inc     rdi
.atoi_lp:
    movzx   rdx, byte [rdi]
    test    dl, dl
    jz      .atoi_done
    ; Convertit chiffre/lettre → valeur numérique dans rdx.
    cmp     dl, '0'
    jb      .atoi_done
    cmp     dl, '9'
    jle     .atoi_num
    cmp     dl, 'A'
    jb      .atoi_done
    cmp     dl, 'Z'
    jle     .atoi_maj
    cmp     dl, 'a'
    jb      .atoi_done
    cmp     dl, 'z'
    jg      .atoi_done
    sub     dl, 'a' - 10
    jmp     .atoi_chk
.atoi_maj:
    sub     dl, 'A' - 10
    jmp     .atoi_chk
.atoi_num:
    sub     dl, '0'
.atoi_chk:
    movzx   rdx, dl
    cmp     rdx, rsi        ; chiffre >= base → arrêt
    jae     .atoi_done
    imul    rax, rsi
    add     rax, rdx
    inc     rdi
    jmp     .atoi_lp
.atoi_done:
    test    r8, r8
    jz      .atoi_pos2
    neg     rax
.atoi_pos2:
    ret

.print_num:     ; (rdi=val i64) — affiche l'entier décimal sur stdout
    ;   Réserve un buffer sur la pile (22 octets suffisent pour i64).
    sub     rsp, 24
    mov     rsi, rsp        ; buf
    mov     rdx, 10
    call    asm.itoa        ; rax = longueur
    mov     rdx, rax
    mov     rsi, rsp
    mov     rdi, 1
    mov     rax, 1
    syscall
    add     rsp, 24
    ret

.print_hex:     ; (rdi=val u64) — affiche "0x<VALEUR_HEX>" sur stdout
    push    rbx
    mov     rbx, rdi
    lea     rsi, [rel _asmstd_0x]
    mov     rdi, 1
    mov     rdx, 2
    mov     rax, 1
    syscall
    sub     rsp, 24
    mov     rdi, rbx
    mov     rsi, rsp
    mov     rdx, 16
    call    asm.itoa
    mov     rdx, rax
    mov     rsi, rsp
    mov     rdi, 1
    mov     rax, 1
    syscall
    add     rsp, 24
    pop     rbx
    ret

; ---- Données internes (lecture seule) --------------------------------

section .rodata
_asmstd_nl: db 10           ; '\n'
_asmstd_0x: db '0', 'x'

section .text               ; rétablit .text pour le code suivant l'%include

%endif
