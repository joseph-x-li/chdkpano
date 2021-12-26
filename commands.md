# Multicam Commands

Note: For Ubuntu, need to modify startup script since gphoto intercepts the connection to the cameras. See

https://chdk.setepontos.com/index.php?topic=10664.20 or 
https://tools.assembla.com/svn/chdkptp/trunk/INSTALL.TXT (search for "gphoto")


```
!mc=require('multicam')
!mc:connect()
!mc:start()
!return mc:cmdwait('rec')
!return mc:cmdwait('preshoot')
!return mc:cmdwait('shoot')
!return mc:cmdwait('play')
!mc:cmd('exit') # lets you use regular commmands
```

Debugging Stuff
```
!mc:list_all()
!mc:list_sel() 
!mc:save_list()
!mc:load_list()
!mc:cmdwait('id')
```

File Movement  
https://chdk.setepontos.com/index.php?topic=13917.0

```
!mc:download_images({dst='${id}/${subdir}/${name}',pretend=true})
```

Power
```
!mc:cmdwait('call post_levent_to_ui("PressPowerButton")')
```

Photo Settings
```
!mc:cmdwait('call set_zoom(0)')
!mc:cmdwait('call set_zoom(127)')
```


# Regular Commands

```
list
rec
=set_zoom(60)
```
