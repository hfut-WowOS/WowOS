<div align='center'>
    <h1>Welcome to WowOS!</h1>
    <br>
    <picture>
        <img alt="NexT preview" src="docs/figures/WowOS.png">
    </picture>
    <div>
        <a href='#'>English</a> |
        <a href='docs/figures/Readme_ch.md'>Chinese</a>
        <!-- <div>
            <sub>Licensed under <a href='https://creativecommons.org/licenses/by-sa/4.0'>CC BY-SA 4.0<a/>.</sub>
        </div> -->
    </div>
    <br>
</div>


# How to use

## build kernel

```shell
make all
```

## run on qemu

```shell
make run
```
## run in docker

```shell
docker run --rm -it -v .:/code --entrypoint bash alphamj/os-contest:v7.3
```