; ======================================================================
;  asmstd.inc — bibliothèque standard pédagogique pour ASM Studio
;  Linux x86-64 (NASM)
; ======================================================================
;
; À QUOI ÇA SERT
;   Écrire « bonjour » en assembleur nu demande de connaître le numéro du
;   syscall write (1), l'ordre exact de ses arguments, et de compter soi-même
;   la longueur de la chaîne. Ce n'est pas de l'assembleur : c'est de la
;   paperasse, et elle décourage avant d'avoir rien appris.
;
;   asmstd met un nom lisible sur cette paperasse. Au lieu de :
;
;       mov  rax, 1          ; quel numéro, déjà ?
;       mov  rdi, 1
;       lea  rsi, [msg]
;       mov  rdx, 13         ; longueur comptée à la main
;       syscall
;
;   on écrit :
;
;       lea  rdi, [msg]
;       call asm.print       ; la longueur est calculée pour vous
;
;   Le programme reste du vrai assembleur, exécuté par le vrai noyau : rien
;   n'est simulé. On déplace seulement l'effort vers ce qu'on veut apprendre.
;
;   Chaque fonction reste lisible dans ce fichier : quand une notion est
;   acquise, on peut ouvrir asmstd.inc et voir exactement ce qu'elle fait.
;
; UTILISATION
;   %include "asmstd.inc"     ; une seule fois (protégé par %ifndef)
;   Activez « Bibliothèque asmstd » dans Fichier ▸ Préférences pour qu'ASM
;   Studio trouve ce fichier depuis n'importe quel dossier.
;
; CONVENTION D'APPEL (System V AMD64 — celle de Linux)
;   Arguments  : rdi  rsi  rdx  rcx  r8   r9
;   Retour     : rax  (négatif = -errno pour les enveloppes de syscall)
;   Préservés  : rbx  rbp  r12..r15   (restaurés par la fonction appelée)
;   Écrasés    : rax  rcx  rdx  rsi  rdi  r8..r11
;
;   Piège classique : le noyau attend le 4e argument dans r10, pas rcx.
;   Les enveloppes concernées (asm.mmap, asm.wait4, asm.send…) font le
;   « mov r10, rcx » pour vous — passez vos arguments normalement.
;
; ----------------------------------------------------------------------
;  INDEX
; ----------------------------------------------------------------------
;  Sortie          print  println  print_num  print_hex  print_bin
;                  print_char  print_stderr  newline
;  Entrée          read_line
;  Fichiers        open  close  read  write  lseek  stat  fstat  lstat
;                  access  truncate  ftruncate  unlink  rename  link
;                  symlink  readlink  chmod  dup  dup2  pipe  fcntl  ioctl
;                  poll  getdents64
;  Dossiers        mkdir  rmdir  chdir  getcwd
;  Processus       fork  execve  exit  exit_group  wait4  waitpid  kill
;                  getpid  getppid  getuid  geteuid  getgid  getegid
;  Mémoire         mmap  munmap  mremap  mprotect  brk
;                  memcpy  memmove  memset  memcmp
;  Réseau          socket  bind  listen  accept  connect  send  recv
;                  shutdown  setsockopt
;  Temps           nanosleep  gettimeofday  clock_gettime
;  Chaînes         strlen  strcpy  strncpy  strcat  strcmp  strncmp
;                  strchr  strrchr  strstr  str_upper  str_lower
;                  str_reverse  str_count
;  Caractères      is_digit  is_alpha  is_alnum  is_space  is_upper
;                  is_lower  to_upper  to_lower
;  Nombres         atoi  itoa  abs  sign  min  max  clamp  divmod
;                  gcd  lcm  pow
;  Tableaux (q)    arr_sum  arr_max  arr_min  arr_find  arr_reverse
;                  arr_sort
;  Vérification    assert_eq
;
;  Chaque fonction porte en commentaire sa signature exacte, juste au-dessus
;  de son code. Cherchez « .nom: » dans ce fichier.
; ======================================================================

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

; ======================================================================
;  CARACTÈRES — classification et conversion (sur al / dil)
; ======================================================================
;   Toutes prennent le caractère dans dil et renvoient dans rax.
;   Les prédicats renvoient 1 (vrai) ou 0 (faux) : utilisables tels quels
;   avec « test rax, rax » puis « jz ».

.is_digit:      ; (dil=car) -> rax = 1 si '0'..'9', sinon 0
    xor     rax, rax
    cmp     dil, '0'
    jb      .isd_no
    cmp     dil, '9'
    ja      .isd_no
    inc     rax
.isd_no:
    ret

.is_upper:      ; (dil=car) -> rax = 1 si 'A'..'Z'
    xor     rax, rax
    cmp     dil, 'A'
    jb      .isu_no
    cmp     dil, 'Z'
    ja      .isu_no
    inc     rax
.isu_no:
    ret

.is_lower:      ; (dil=car) -> rax = 1 si 'a'..'z'
    xor     rax, rax
    cmp     dil, 'a'
    jb      .isl_no
    cmp     dil, 'z'
    ja      .isl_no
    inc     rax
.isl_no:
    ret

.is_alpha:      ; (dil=car) -> rax = 1 si lettre (majuscule ou minuscule)
    call    asm.is_upper
    test    rax, rax
    jnz     .isa_yes
    call    asm.is_lower
    ret
.isa_yes:
    ret

.is_alnum:      ; (dil=car) -> rax = 1 si lettre ou chiffre
    call    asm.is_alpha
    test    rax, rax
    jnz     .isan_yes
    call    asm.is_digit
    ret
.isan_yes:
    ret

.is_space:      ; (dil=car) -> rax = 1 si espace, tabulation, \n, \r, \f, \v
    xor     rax, rax
    cmp     dil, ' '
    je      .iss_yes
    cmp     dil, 9          ; \t
    jb      .iss_no
    cmp     dil, 13         ; \r
    ja      .iss_no
.iss_yes:
    inc     rax
.iss_no:
    ret

.to_upper:      ; (dil=car) -> rax = car en majuscule (inchangé si pas 'a'..'z')
    movzx   rax, dil
    cmp     dil, 'a'
    jb      .tu_done
    cmp     dil, 'z'
    ja      .tu_done
    sub     rax, 32
.tu_done:
    ret

.to_lower:      ; (dil=car) -> rax = car en minuscule (inchangé si pas 'A'..'Z')
    movzx   rax, dil
    cmp     dil, 'A'
    jb      .tl_done
    cmp     dil, 'Z'
    ja      .tl_done
    add     rax, 32
.tl_done:
    ret

; ======================================================================
;  CHAÎNES — au-delà de strlen / strcpy / strcmp
; ======================================================================

.strncmp:       ; (rdi=a, rsi=b, rdx=n) -> rax = 0 si les n premiers octets
                ;   sont égaux, sinon différence (a[i] - b[i]) signée
    xor     rcx, rcx
.sncmp_lp:
    cmp     rcx, rdx
    jae     .sncmp_eq       ; n octets comparés sans différence
    movzx   rax, byte [rdi + rcx]
    movzx   r8,  byte [rsi + rcx]
    sub     rax, r8
    jnz     .sncmp_done     ; différence trouvée
    cmp     byte [rdi + rcx], 0
    je      .sncmp_eq       ; fin des deux chaînes
    inc     rcx
    jmp     .sncmp_lp
.sncmp_eq:
    xor     rax, rax
.sncmp_done:
    ret

.strchr:        ; (rdi=str, sil=car) -> rax = pointeur sur la 1re occurrence,
                ;   ou 0 si absente. Cherche aussi le '\0' si sil = 0.
    mov     rax, rdi
.schr_lp:
    movzx   rcx, byte [rax]
    cmp     cl, sil
    je      .schr_done
    test    cl, cl
    jz      .schr_none
    inc     rax
    jmp     .schr_lp
.schr_none:
    xor     rax, rax
.schr_done:
    ret

.strrchr:       ; (rdi=str, sil=car) -> rax = dernière occurrence, ou 0
    xor     rax, rax        ; meilleur candidat trouvé
    mov     rcx, rdi
.srchr_lp:
    movzx   r8, byte [rcx]
    cmp     r8b, sil
    jne     .srchr_next
    mov     rax, rcx        ; on retient, mais on continue
.srchr_next:
    test    r8b, r8b
    jz      .srchr_done
    inc     rcx
    jmp     .srchr_lp
.srchr_done:
    ret

.strstr:        ; (rdi=botte, rsi=aiguille) -> rax = pointeur sur la 1re
                ;   occurrence de l'aiguille, ou 0. Aiguille vide -> botte.
    cmp     byte [rsi], 0
    je      .sstr_found_here
    mov     rax, rdi
.sstr_outer:
    cmp     byte [rax], 0
    je      .sstr_none
    xor     rcx, rcx
.sstr_inner:
    movzx   r8, byte [rsi + rcx]
    test    r8b, r8b
    jz      .sstr_done      ; aiguille épuisée : trouvée à rax
    movzx   r9, byte [rax + rcx]
    cmp     r8b, r9b
    jne     .sstr_next
    inc     rcx
    jmp     .sstr_inner
.sstr_next:
    inc     rax
    jmp     .sstr_outer
.sstr_none:
    xor     rax, rax
    ret
.sstr_found_here:
    mov     rax, rdi
.sstr_done:
    ret

.strcat:        ; (rdi=dst, rsi=src) -> rax = dst
                ;   Ajoute src à la fin de dst. dst doit avoir la place.
    push    rbx
    mov     rbx, rdi
    call    asm.strlen      ; rax = longueur de dst
    lea     rdi, [rbx + rax]
    call    asm.strcpy      ; recopie src au bout
    mov     rax, rbx
    pop     rbx
    ret

.strncpy:       ; (rdi=dst, rsi=src, rdx=n) -> rax = dst
                ;   Copie au plus n octets ; complète par des '\0'.
    mov     rax, rdi
    xor     rcx, rcx
.sncpy_lp:
    cmp     rcx, rdx
    jae     .sncpy_done
    movzx   r8, byte [rsi + rcx]
    mov     byte [rdi + rcx], r8b
    test    r8b, r8b
    jz      .sncpy_pad      ; source finie : on complète de zéros
    inc     rcx
    jmp     .sncpy_lp
.sncpy_pad:
    inc     rcx
    cmp     rcx, rdx
    jae     .sncpy_done
    mov     byte [rdi + rcx], 0
    jmp     .sncpy_pad
.sncpy_done:
    ret

.str_upper:     ; (rdi=str) -> rax = str, converti en majuscules SUR PLACE
    push    rbx
    mov     rbx, rdi
    xor     rcx, rcx
.supr_lp:
    movzx   rdi, byte [rbx + rcx]
    test    dil, dil
    jz      .supr_done
    push    rcx
    call    asm.to_upper
    pop     rcx
    mov     byte [rbx + rcx], al
    inc     rcx
    jmp     .supr_lp
.supr_done:
    mov     rax, rbx
    pop     rbx
    ret

.str_lower:     ; (rdi=str) -> rax = str, converti en minuscules SUR PLACE
    push    rbx
    mov     rbx, rdi
    xor     rcx, rcx
.slwr_lp:
    movzx   rdi, byte [rbx + rcx]
    test    dil, dil
    jz      .slwr_done
    push    rcx
    call    asm.to_lower
    pop     rcx
    mov     byte [rbx + rcx], al
    inc     rcx
    jmp     .slwr_lp
.slwr_done:
    mov     rax, rbx
    pop     rbx
    ret

.str_reverse:   ; (rdi=str) -> rax = str, inversée SUR PLACE
    push    rbx
    mov     rbx, rdi
    call    asm.strlen      ; rax = longueur
    mov     rdi, rbx
    test    rax, rax
    jz      .srev_done
    lea     rcx, [rbx + rax - 1]    ; fin
    mov     rdx, rbx                ; début
.srev_lp:
    cmp     rdx, rcx
    jae     .srev_done
    movzx   r8,  byte [rdx]
    movzx   r9,  byte [rcx]
    mov     byte [rdx], r9b
    mov     byte [rcx], r8b
    inc     rdx
    dec     rcx
    jmp     .srev_lp
.srev_done:
    mov     rax, rbx
    pop     rbx
    ret

.str_count:     ; (rdi=str, sil=car) -> rax = nombre d'occurrences de car
    xor     rax, rax
    xor     rcx, rcx
.scnt_lp:
    movzx   r8, byte [rdi + rcx]
    test    r8b, r8b
    jz      .scnt_done
    cmp     r8b, sil
    jne     .scnt_next
    inc     rax
.scnt_next:
    inc     rcx
    jmp     .scnt_lp
.scnt_done:
    ret

; ======================================================================
;  MÉMOIRE — au-delà de memcpy / memset
; ======================================================================

.memcmp:        ; (rdi=a, rsi=b, rdx=n) -> rax = 0 si identiques,
                ;   sinon différence signée du 1er octet qui diffère
    xor     rcx, rcx
.mcmp_lp:
    cmp     rcx, rdx
    jae     .mcmp_eq
    movzx   rax, byte [rdi + rcx]
    movzx   r8,  byte [rsi + rcx]
    sub     rax, r8
    jnz     .mcmp_done
    inc     rcx
    jmp     .mcmp_lp
.mcmp_eq:
    xor     rax, rax
.mcmp_done:
    ret

.memmove:       ; (rdi=dst, rsi=src, rdx=n) -> rax = dst
                ;   Contrairement à memcpy, gère les zones qui SE CHEVAUCHENT :
                ;   copie d'arrière en avant quand dst est au-dessus de src.
    mov     rax, rdi
    test    rdx, rdx
    jz      .mmv_done
    cmp     rdi, rsi
    jbe     .mmv_fwd        ; dst <= src : copie normale
    ;   dst > src : si les zones se chevauchent, copier en avant écraserait
    ;   la source. On part donc de la fin.
    mov     rcx, rdx
.mmv_bwd:
    dec     rcx
    movzx   r8, byte [rsi + rcx]
    mov     byte [rdi + rcx], r8b
    test    rcx, rcx
    jnz     .mmv_bwd
    ret
.mmv_fwd:
    xor     rcx, rcx
.mmv_flp:
    movzx   r8, byte [rsi + rcx]
    mov     byte [rdi + rcx], r8b
    inc     rcx
    cmp     rcx, rdx
    jb      .mmv_flp
.mmv_done:
    ret

; ======================================================================
;  ARITHMÉTIQUE — entiers signés 64 bits
; ======================================================================

.abs:           ; (rdi=n i64) -> rax = |n|
    mov     rax, rdi
    cqo                     ; rdx = signe étendu de rax
    xor     rax, rdx        ; si négatif : complément à un…
    sub     rax, rdx        ; …puis +1  => valeur absolue
    ret

.sign:          ; (rdi=n i64) -> rax = -1, 0 ou 1
    xor     rax, rax
    test    rdi, rdi
    jz      .sgn_done
    mov     rax, 1
    jg      .sgn_done
    mov     rax, -1
.sgn_done:
    ret

.min:           ; (rdi=a, rsi=b) -> rax = plus petit (SIGNÉ)
    mov     rax, rdi
    cmp     rsi, rax
    cmovl   rax, rsi
    ret

.max:           ; (rdi=a, rsi=b) -> rax = plus grand (SIGNÉ)
    mov     rax, rdi
    cmp     rsi, rax
    cmovg   rax, rsi
    ret

.clamp:         ; (rdi=n, rsi=bas, rdx=haut) -> rax = n ramené dans [bas, haut]
    mov     rax, rdi
    cmp     rax, rsi
    cmovl   rax, rsi
    cmp     rax, rdx
    cmovg   rax, rdx
    ret

.divmod:        ; (rdi=a, rsi=b) -> rax = a/b, rdx = a%b  (division SIGNÉE)
                ;   Renvoie rax=0, rdx=0 si b vaut 0 — plutôt que de laisser le
                ;   processeur lever une exception qui tuerait le programme.
    test    rsi, rsi
    jz      .dm_zero
    mov     rax, rdi
    cqo                     ; étend le signe de rax dans rdx (obligatoire)
    idiv    rsi
    ret
.dm_zero:
    xor     rax, rax
    xor     rdx, rdx
    ret

.gcd:           ; (rdi=a, rsi=b) -> rax = PGCD(|a|, |b|)  — algorithme d'Euclide
    push    rbx
    call    asm.abs
    mov     rbx, rax        ; rbx = |a|
    mov     rdi, rsi
    call    asm.abs         ; rax = |b|
.gcd_lp:
    test    rax, rax
    jz      .gcd_done
    ;   (rbx, rax) <- (rax, rbx mod rax)
    mov     rcx, rax
    mov     rax, rbx
    cqo
    idiv    rcx             ; rdx = rbx mod rcx
    mov     rbx, rcx
    mov     rax, rdx
    jmp     .gcd_lp
.gcd_done:
    mov     rax, rbx
    pop     rbx
    ret

.lcm:           ; (rdi=a, rsi=b) -> rax = PPCM(|a|, |b|), ou 0 si a ou b = 0
    push    rbx
    push    r12
    mov     rbx, rdi
    mov     r12, rsi
    test    rbx, rbx
    jz      .lcm_zero
    test    r12, r12
    jz      .lcm_zero
    call    asm.gcd         ; rax = pgcd
    mov     rcx, rax
    mov     rdi, rbx
    call    asm.abs
    cqo
    idiv    rcx             ; rax = |a| / pgcd
    mov     rbx, rax
    mov     rdi, r12
    call    asm.abs
    imul    rax, rbx
    jmp     .lcm_done
.lcm_zero:
    xor     rax, rax
.lcm_done:
    pop     r12
    pop     rbx
    ret

.pow:           ; (rdi=base, rsi=exposant >= 0) -> rax = base^exposant
                ;   Exponentiation rapide : O(log n) multiplications.
    mov     rax, 1
    test    rsi, rsi
    jle     .pow_done       ; exposant nul ou négatif -> 1
    mov     rcx, rdi        ; puissance courante de la base
.pow_lp:
    test    rsi, 1
    jz      .pow_sq
    imul    rax, rcx
.pow_sq:
    shr     rsi, 1
    jz      .pow_done
    imul    rcx, rcx
    jmp     .pow_lp
.pow_done:
    ret

; ======================================================================
;  TABLEAUX de qwords (entiers 64 bits)
; ======================================================================
;   Partout : rdi = adresse du tableau, rsi = nombre d'éléments.

.arr_sum:       ; (rdi=tab, rsi=n) -> rax = somme des éléments
    xor     rax, rax
    xor     rcx, rcx
.asum_lp:
    cmp     rcx, rsi
    jae     .asum_done
    add     rax, [rdi + rcx * 8]
    inc     rcx
    jmp     .asum_lp
.asum_done:
    ret

.arr_max:       ; (rdi=tab, rsi=n) -> rax = plus grand élément (0 si n = 0)
    xor     rax, rax
    test    rsi, rsi
    jz      .amax_done
    mov     rax, [rdi]
    mov     rcx, 1
.amax_lp:
    cmp     rcx, rsi
    jae     .amax_done
    mov     r8, [rdi + rcx * 8]
    cmp     r8, rax
    cmovg   rax, r8
    inc     rcx
    jmp     .amax_lp
.amax_done:
    ret

.arr_min:       ; (rdi=tab, rsi=n) -> rax = plus petit élément (0 si n = 0)
    xor     rax, rax
    test    rsi, rsi
    jz      .amin_done
    mov     rax, [rdi]
    mov     rcx, 1
.amin_lp:
    cmp     rcx, rsi
    jae     .amin_done
    mov     r8, [rdi + rcx * 8]
    cmp     r8, rax
    cmovl   rax, r8
    inc     rcx
    jmp     .amin_lp
.amin_done:
    ret

.arr_find:      ; (rdi=tab, rsi=n, rdx=valeur) -> rax = index, ou -1 si absente
    xor     rcx, rcx
.afind_lp:
    cmp     rcx, rsi
    jae     .afind_none
    cmp     [rdi + rcx * 8], rdx
    je      .afind_done
    inc     rcx
    jmp     .afind_lp
.afind_none:
    mov     rax, -1
    ret
.afind_done:
    mov     rax, rcx
    ret

.arr_reverse:   ; (rdi=tab, rsi=n) -> rax = tab, inversé SUR PLACE
    mov     rax, rdi
    test    rsi, rsi
    jz      .arev_done
    xor     rcx, rcx        ; index de gauche
    mov     rdx, rsi
    dec     rdx             ; index de droite
.arev_lp:
    cmp     rcx, rdx
    jae     .arev_done
    mov     r8, [rdi + rcx * 8]
    mov     r9, [rdi + rdx * 8]
    mov     [rdi + rcx * 8], r9
    mov     [rdi + rdx * 8], r8
    inc     rcx
    dec     rdx
    jmp     .arev_lp
.arev_done:
    ret

.arr_sort:      ; (rdi=tab, rsi=n) -> rax = tab, trié en ORDRE CROISSANT
                ;   Tri à bulles : O(n²), mais court et facile à suivre pas à
                ;   pas — c'est le but ici, pas la vitesse.
    mov     rax, rdi
    cmp     rsi, 2
    jb      .asort_done
    mov     r10, rsi
    dec     r10             ; limite du balayage
.asort_pass:
    xor     r11, r11        ; drapeau « un échange a eu lieu »
    xor     rcx, rcx
.asort_lp:
    cmp     rcx, r10
    jae     .asort_check
    mov     r8, [rdi + rcx * 8]
    mov     r9, [rdi + rcx * 8 + 8]
    cmp     r8, r9
    jle     .asort_next
    mov     [rdi + rcx * 8], r9
    mov     [rdi + rcx * 8 + 8], r8
    mov     r11, 1
.asort_next:
    inc     rcx
    jmp     .asort_lp
.asort_check:
    test    r11, r11
    jz      .asort_done     ; passe sans échange : c'est trié
    dec     r10
    jnz     .asort_pass
.asort_done:
    ret

; ======================================================================
;  ENTRÉES / SORTIES de confort
; ======================================================================

.print_char:    ; (dil=car) — écrit un seul caractère sur stdout
    push    rbp
    mov     rbp, rsp
    sub     rsp, 16
    mov     [rsp], dil
    mov     rdi, 1
    mov     rsi, rsp
    mov     rdx, 1
    mov     rax, 1
    syscall
    leave
    ret

.print_bin:     ; (rdi=val u64, rsi=bits à afficher, 1..64) — motif binaire
                ;   Affiche les `rsi` bits de poids faible, du plus fort au plus
                ;   faible. Utile pour VOIR ce que font shl, and, xor…
    push    rbx
    push    r12
    mov     rbx, rdi
    mov     r12, rsi
    cmp     r12, 1
    jb      .pbin_done
    cmp     r12, 64
    jbe     .pbin_lp
    mov     r12, 64
.pbin_lp:
    dec     r12
    mov     rcx, r12
    mov     rax, rbx
    shr     rax, cl
    and     rax, 1
    add     rax, '0'
    mov     rdi, rax
    call    asm.print_char
    test    r12, r12
    jnz     .pbin_lp
.pbin_done:
    pop     r12
    pop     rbx
    ret

.read_line:     ; (rdi=buf, rsi=taille) -> rax = longueur lue (sans le \n)
                ;   Lit une ligne sur stdin. Le \n final n'est PAS conservé ;
                ;   la chaîne est terminée par '\0'. Renvoie 0 sur fin de
                ;   fichier, ou -errno en cas d'erreur.
    push    rbx
    push    r12
    mov     rbx, rdi
    mov     r12, rsi
    test    r12, r12
    jz      .rdl_empty
    dec     r12             ; garde la place du '\0'
    xor     rdi, rdi        ; fd = stdin
    mov     rsi, rbx
    mov     rdx, r12
    mov     rax, 0          ; sys_read
    syscall
    test    rax, rax
    jle     .rdl_done       ; 0 = EOF, < 0 = erreur
    ;   Retire le saut de ligne final s'il est présent.
    cmp     byte [rbx + rax - 1], 10
    jne     .rdl_term
    dec     rax
.rdl_term:
    mov     byte [rbx + rax], 0
    jmp     .rdl_done
.rdl_empty:
    xor     rax, rax
.rdl_done:
    pop     r12
    pop     rbx
    ret

; ======================================================================
;  VÉRIFICATION — pour écrire des programmes qui se contrôlent eux-mêmes
; ======================================================================

.assert_eq:     ; (rdi=obtenu, rsi=attendu, rdx=nom) -> rax = 1 si égaux
                ;   Affiche « nom : OK » ou « nom : ÉCHEC (obtenu N, attendu M) »
                ;   et renvoie 1 ou 0. N'interrompt PAS le programme : à
                ;   l'appelant de décider quoi faire du résultat.
    push    rbx
    push    r12
    push    r13
    mov     rbx, rdi        ; obtenu
    mov     r12, rsi        ; attendu
    mov     r13, rdx        ; nom

    mov     rdi, r13
    call    asm.print
    mov     rdi, ' '
    call    asm.print_char
    mov     rdi, ':'
    call    asm.print_char
    mov     rdi, ' '
    call    asm.print_char

    cmp     rbx, r12
    jne     .aeq_fail
    lea     rdi, [_asmstd_ok]
    call    asm.print
    call    asm.newline
    mov     rax, 1
    jmp     .aeq_done
.aeq_fail:
    lea     rdi, [_asmstd_ko]
    call    asm.print
    mov     rdi, rbx
    call    asm.print_num
    lea     rdi, [_asmstd_exp]
    call    asm.print
    mov     rdi, r12
    call    asm.print_num
    mov     rdi, ')'
    call    asm.print_char
    call    asm.newline
    xor     rax, rax
.aeq_done:
    pop     r13
    pop     r12
    pop     rbx
    ret

; ---- Données internes (lecture seule) --------------------------------

section .rodata
_asmstd_nl: db 10           ; '\n'
_asmstd_0x: db '0', 'x'
_asmstd_ok:  db 'OK', 0
_asmstd_ko:  db 'ECHEC (obtenu ', 0
_asmstd_exp: db ', attendu ', 0

section .text               ; rétablit .text pour le code suivant l'%include

%endif
