<div align='center'>
<picture>
  <img alt="NexT preview" src="docs/figures/WowOS.png">
</picture>
  <h1>Welcome to WowOS!</h1>
  <div>
    <a href='#'>English</a> |
    <a href='docs/figures/Readme_ch.md'>Chinese</a>
    <!-- <div>
      <br />
      <sub>Licensed under <a href='https://creativecommons.org/licenses/by-sa/4.0'>CC BY-SA 4.0<a/>.</sub>
    </div> -->
  </div>
  <br /><br />
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